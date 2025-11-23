// 交易所存储系统端到端集成测试
//
// 测试流程：
// 1. 启动 WAL、OLTP、OLAP 完整存储链路
// 2. 模拟大量订单交易
// 3. 验证数据一致性
// 4. 性能统计

use qaexchange::storage::conversion::{ConversionManager, SchedulerConfig, WorkerConfig};
use qaexchange::storage::hybrid::oltp::{OltpHybridConfig, OltpHybridStorage};
use qaexchange::storage::wal::record::WalRecord;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 订单生成器配置
struct OrderGeneratorConfig {
    /// 品种数量
    instruments: Vec<String>,
    /// 每个品种订单数量
    orders_per_instrument: usize,
    /// 并发线程数
    concurrent_threads: usize,
}

impl Default for OrderGeneratorConfig {
    fn default() -> Self {
        Self {
            instruments: vec![
                "rb2501".to_string(),
                "hc2501".to_string(),
                "au2506".to_string(),
            ],
            orders_per_instrument: 10000,
            concurrent_threads: 4,
        }
    }
}

/// 性能统计
#[derive(Debug, Clone)]
struct PerformanceStats {
    /// 总订单数
    total_orders: u64,
    /// 总耗时（毫秒）
    total_duration_ms: u64,
    /// 吞吐量（订单/秒）
    throughput: f64,
    /// 平均延迟（微秒）
    avg_latency_us: f64,
    /// P99 延迟（微秒）
    p99_latency_us: f64,
    /// WAL 写入数
    wal_writes: u64,
    /// SSTable flush 次数
    sstable_flushes: u64,
}

/// 生成模拟订单
fn generate_order(instrument_id: &str, order_id: u64, timestamp: i64) -> WalRecord {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let direction = if rng.gen::<bool>() { 0u8 } else { 1u8 };
    let offset = if rng.gen::<bool>() { 0u8 } else { 1u8 };
    let price = 3500.0 + rng.gen::<f64>() * 100.0;
    let volume = (rng.gen::<f64>() * 10.0 + 1.0).round();

    // 生成 instrument_id 的固定长度字节数组
    let mut instrument_bytes = [0u8; 16];
    let bytes = instrument_id.as_bytes();
    let len = bytes.len().min(16);
    instrument_bytes[..len].copy_from_slice(&bytes[..len]);

    WalRecord::OrderInsert {
        order_id,
        user_id: [1u8; 32],
        instrument_id: instrument_bytes,
        direction,
        offset,
        price,
        volume,
        timestamp,
    }
}

/// 集成测试：完整存储链路
#[test]
#[ignore] // 标记为 ignore，需要手动运行：cargo test --test exchange_integration_test -- --ignored
fn test_full_storage_pipeline() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let test_dir = tempfile::tempdir().unwrap();
    let base_path = test_dir.path().to_path_buf();

    log::info!("========================================");
    log::info!("交易所存储系统集成测试");
    log::info!("测试目录: {:?}", base_path);
    log::info!("========================================");

    // 配置
    let config = OrderGeneratorConfig::default();
    let total_orders = config.instruments.len() * config.orders_per_instrument;

    log::info!("测试配置:");
    log::info!("  - 品种数量: {}", config.instruments.len());
    log::info!("  - 每品种订单: {}", config.orders_per_instrument);
    log::info!("  - 总订单数: {}", total_orders);
    log::info!("  - 并发线程: {}", config.concurrent_threads);

    // ========================================
    // Phase 1: 初始化存储系统
    // ========================================
    log::info!("\n[Phase 1] 初始化存储系统...");

    let mut storages = std::collections::HashMap::new();

    for instrument in &config.instruments {
        // 创建 OLTP HybridStorage
        let storage_config = OltpHybridConfig {
            base_path: base_path.to_str().unwrap().to_string(),
            memtable_size_bytes: 1 * 1024 * 1024, // 1 MB threshold for auto-flush
            estimated_entry_size: 256,
        };

        let storage = Arc::new(OltpHybridStorage::create(instrument, storage_config).unwrap());
        storages.insert(instrument.clone(), storage);

        log::info!("  ✓ 初始化品种 {} 存储", instrument);
    }

    // ========================================
    // Phase 2: 生成并写入订单
    // ========================================
    log::info!("\n[Phase 2] 生成并写入订单...");

    let order_counter = Arc::new(AtomicU64::new(0));
    let mut latencies = Vec::new();

    let start_time = Instant::now();
    let start_timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap();

    // 多线程并发写入
    let mut handles = Vec::new();

    for instrument in &config.instruments {
        let orders_per_thread = config.orders_per_instrument / config.concurrent_threads;
        let storage = storages.get(instrument).unwrap().clone();

        for thread_id in 0..config.concurrent_threads {
            let instrument_clone = instrument.clone();
            let storage_clone = storage.clone();
            let counter = order_counter.clone();
            let start_ts = start_timestamp;

            let handle = std::thread::spawn(move || {
                let mut thread_latencies = Vec::new();

                for i in 0..orders_per_thread {
                    let order_id = (thread_id * orders_per_thread + i) as u64;
                    let timestamp = start_ts + (i as i64 * 1000); // 每微秒一个订单

                    let order = generate_order(&instrument_clone, order_id, timestamp);

                    // 记录延迟
                    let write_start = Instant::now();
                    storage_clone.write(order).unwrap();
                    let latency = write_start.elapsed();
                    thread_latencies.push(latency.as_micros() as u64);

                    counter.fetch_add(1, Ordering::Relaxed);

                    // 每1000个订单打印进度
                    if (i + 1) % 1000 == 0 {
                        let progress = counter.load(Ordering::Relaxed);
                        log::debug!("  进度: {}/{} 订单", progress, total_orders);
                    }
                }

                thread_latencies
            });

            handles.push(handle);
        }
    }

    // 等待所有线程完成
    for handle in handles {
        let thread_latencies = handle.join().unwrap();
        latencies.extend(thread_latencies);
    }

    let write_duration = start_time.elapsed();
    let orders_written = order_counter.load(Ordering::Relaxed);

    log::info!("  ✓ 写入完成");
    log::info!("    - 订单数: {}", orders_written);
    log::info!("    - 耗时: {:.2}s", write_duration.as_secs_f64());
    log::info!(
        "    - 吞吐量: {:.0} 订单/秒",
        orders_written as f64 / write_duration.as_secs_f64()
    );

    // ========================================
    // Phase 3: 检查存储状态（MemTable → SSTable 自动 flush）
    // ========================================
    log::info!("\n[Phase 3] 检查存储状态（MemTable → SSTable 自动 flush）...");

    let mut total_sstables = 0;

    for (instrument, storage) in &storages {
        let stats = storage.stats();
        total_sstables += stats.sstable_count;
        log::info!(
            "  ✓ 品种 {}: MemTable={} 条, SSTable={} 个",
            instrument,
            stats.memtable_entries,
            stats.sstable_count
        );
    }

    log::info!("  ✓ 总共生成 {} 个 SSTable", total_sstables);

    // ========================================
    // Phase 4: 验证数据完整性
    // ========================================
    log::info!("\n[Phase 4] 验证数据完整性...");

    let mut total_read = 0;

    for (instrument, storage) in &storages {
        // 范围查询：读取所有数据
        let records = storage.range_query(i64::MIN, i64::MAX).unwrap();
        total_read += records.len();

        log::info!("  ✓ 品种 {}: 读取 {} 条记录", instrument, records.len());

        // 验证订单数量
        assert_eq!(
            records.len(),
            config.orders_per_instrument,
            "品种 {} 订单数量不匹配",
            instrument
        );
    }

    log::info!("  ✓ 数据完整性验证通过");
    log::info!("    - 写入: {} 条", orders_written);
    log::info!("    - 读取: {} 条", total_read);
    assert_eq!(orders_written as usize, total_read);

    // ========================================
    // Phase 5: 性能统计
    // ========================================
    log::info!("\n[Phase 5] 性能统计...");

    // 计算延迟统计
    latencies.sort();
    let avg_latency = latencies.iter().sum::<u64>() as f64 / latencies.len() as f64;
    let p99_idx = (latencies.len() as f64 * 0.99) as usize;
    let p99_latency = latencies[p99_idx];
    let p999_idx = (latencies.len() as f64 * 0.999) as usize;
    let p999_latency = latencies[p999_idx];

    let stats = PerformanceStats {
        total_orders: orders_written,
        total_duration_ms: write_duration.as_millis() as u64,
        throughput: orders_written as f64 / write_duration.as_secs_f64(),
        avg_latency_us: avg_latency,
        p99_latency_us: p99_latency as f64,
        wal_writes: orders_written, // 每个订单一次 WAL
        sstable_flushes: total_sstables as u64,
    };

    log::info!("========================================");
    log::info!("性能统计报告");
    log::info!("========================================");
    log::info!("吞吐量:");
    log::info!("  - 订单总数: {}", stats.total_orders);
    log::info!("  - 总耗时: {} ms", stats.total_duration_ms);
    log::info!("  - 吞吐量: {:.0} 订单/秒", stats.throughput);
    log::info!("");
    log::info!("延迟分布:");
    log::info!("  - 平均延迟: {:.2} μs", stats.avg_latency_us);
    log::info!("  - P99 延迟: {:.2} μs", stats.p99_latency_us);
    log::info!("  - P999 延迟: {:.2} μs", p999_latency);
    log::info!("");
    log::info!("存储统计:");
    log::info!("  - WAL 写入: {} 次", stats.wal_writes);
    log::info!("  - SSTable Flush: {} 次", stats.sstable_flushes);
    log::info!("========================================");

    // 性能断言 (debug/release 模式分别验证)
    #[cfg(not(debug_assertions))]
    {
        // Release 模式: 高性能预期
        assert!(
            stats.throughput > 10000.0,
            "Release: 吞吐量应该 > 10K 订单/秒"
        );
        assert!(stats.p99_latency_us < 1000.0, "Release: P99 延迟应该 < 1ms");
    }

    #[cfg(debug_assertions)]
    {
        // Debug 模式: 宽松预期 (性能约为 release 的 1/10)
        assert!(stats.throughput > 500.0, "Debug: 吞吐量应该 > 500 订单/秒");
        assert!(
            stats.p99_latency_us < 100000.0,
            "Debug: P99 延迟应该 < 100ms"
        );
        log::warn!(
            "⚠️  Debug 模式性能: {:.0} 订单/秒, P99={:.2}ms",
            stats.throughput,
            stats.p99_latency_us / 1000.0
        );
        log::warn!("💡 使用 --release 模式进行实际性能验证");
    }

    log::info!("\n✅ 集成测试通过！");
}

/// 集成测试：OLTP → OLAP 转换
#[test]
#[ignore] // 标记为 ignore，需要手动运行
fn test_oltp_to_olap_conversion() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let test_dir = tempfile::tempdir().unwrap();
    let base_path = test_dir.path().to_path_buf();

    log::info!("========================================");
    log::info!("OLTP → OLAP 转换集成测试");
    log::info!("========================================");

    // ========================================
    // Phase 1: 准备 OLTP 数据
    // ========================================
    log::info!("\n[Phase 1] 准备 OLTP 数据...");

    let instrument = "rb2501";

    // 创建 OLTP HybridStorage，使用小的 memtable 以触发自动 flush
    let storage_config = OltpHybridConfig {
        base_path: base_path.to_str().unwrap().to_string(),
        memtable_size_bytes: 500 * 1024, // 500 KB，容易触发 flush
        estimated_entry_size: 256,
    };

    let storage = Arc::new(OltpHybridStorage::create(instrument, storage_config).unwrap());

    // 写入测试数据
    let order_count = 5000;
    let start_timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap();

    for i in 0..order_count {
        let timestamp = start_timestamp + (i as i64 * 1000);
        let order = generate_order(instrument, i as u64, timestamp);
        storage.write(order).unwrap();
    }

    log::info!("  ✓ 写入 {} 条 OLTP 记录", order_count);

    // 检查 SSTable 状态（自动 flush）
    let stats = storage.stats();
    log::info!("  ✓ 生成 {} 个 OLTP SSTable", stats.sstable_count);

    // ========================================
    // Phase 2: 启动 OLAP 转换系统
    // ========================================
    log::info!("\n[Phase 2] 启动 OLAP 转换系统...");

    let metadata_path = base_path.join("conversion_metadata.json");

    // 配置：快速转换测试
    let mut scheduler_config = SchedulerConfig::default();
    scheduler_config.scan_interval_secs = 1; // 1秒扫描一次
    scheduler_config.min_sstables_per_batch = 1; // 最少1个文件
    scheduler_config.min_sstable_age_secs = 0; // 不检查年龄

    let worker_config = WorkerConfig {
        worker_count: 2,
        batch_read_size: 10000,
        delete_source_after_success: false, // 测试中保留源文件
        source_retention_secs: 0,
    };

    let mut conversion_manager = ConversionManager::new(
        base_path.clone(),
        metadata_path,
        scheduler_config,
        worker_config,
    )
    .unwrap();

    log::info!("  ✓ 转换系统已配置");

    // ========================================
    // Phase 3: 手动触发转换
    // ========================================
    log::info!("\n[Phase 3] 手动触发转换...");

    // 收集所有 OLTP SSTable
    let instrument_path = base_path.join(instrument);
    let oltp_dir = instrument_path.join("oltp");
    let mut sstables = Vec::new();

    if oltp_dir.exists() {
        for entry in std::fs::read_dir(&oltp_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map(|e| e == "rkyv").unwrap_or(false) {
                sstables.push(path);
            }
        }
    }

    log::info!("  发现 {} 个 OLTP SSTable", sstables.len());

    if !sstables.is_empty() {
        conversion_manager
            .trigger_conversion(instrument, sstables)
            .unwrap();
        log::info!("  ✓ 转换任务已提交");

        // 启动转换系统
        conversion_manager.start();

        // 等待转换完成（轮询状态）
        let mut attempts = 0;
        let max_attempts = 30; // 最多等待30秒

        loop {
            std::thread::sleep(Duration::from_secs(1));
            attempts += 1;

            let stats = conversion_manager.get_stats();
            log::info!(
                "  转换状态: Success={}, Pending={}, Converting={}, Failed={}",
                stats.success,
                stats.pending,
                stats.converting,
                stats.failed
            );

            if stats.success > 0 {
                log::info!("  ✓ 转换完成");
                break;
            }

            if attempts >= max_attempts {
                panic!("转换超时");
            }
        }
    }

    // ========================================
    // Phase 4: 验证 OLAP 数据
    // ========================================
    log::info!("\n[Phase 4] 验证 OLAP 数据...");

    let olap_dir = instrument_path.join("olap");
    assert!(olap_dir.exists(), "OLAP 目录应该存在");

    // 检查 Parquet 文件
    let mut parquet_files = Vec::new();
    for entry in std::fs::read_dir(&olap_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map(|e| e == "parquet").unwrap_or(false) {
            parquet_files.push(path);
        }
    }

    log::info!("  发现 {} 个 Parquet 文件", parquet_files.len());
    assert!(!parquet_files.is_empty(), "应该生成至少1个 Parquet 文件");

    // 读取并验证 Parquet 数据
    use qaexchange::storage::sstable::olap_parquet::ParquetSSTable;

    for parquet_path in &parquet_files {
        let sstable = ParquetSSTable::open(parquet_path).unwrap();
        let metadata = sstable.metadata();

        log::info!("  Parquet 文件: {:?}", parquet_path.file_name().unwrap());
        log::info!("    - 记录数: {}", metadata.entry_count);
        log::info!(
            "    - 时间范围: [{}, {}]",
            metadata.min_timestamp,
            metadata.max_timestamp
        );
        log::info!("    - 文件大小: {} bytes", metadata.file_size);

        // 验证记录数
        assert!(metadata.entry_count > 0, "Parquet 文件应该包含数据");
    }

    log::info!("  ✓ OLAP 数据验证通过");
    log::info!("\n✅ OLTP → OLAP 转换测试通过！");
}
