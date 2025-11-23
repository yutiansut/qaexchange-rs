// OLTP Storage Benchmark
//
// 性能基准测试套件
//
// 使用方法:
//   cargo bench --bench oltp_storage_bench

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use qaexchange::storage::hybrid::oltp::{OltpHybridConfig, OltpHybridStorage};
use qaexchange::storage::wal::record::WalRecord;

// 配置
const WARMUP_ITERATIONS: usize = 100;
const BENCH_ITERATIONS: usize = 1000;
const CONCURRENT_INSTRUMENTS: usize = 10;

fn create_test_record(order_id: u64, timestamp: i64) -> WalRecord {
    WalRecord::OrderInsert {
        order_id,
        user_id: [1u8; 32],
        instrument_id: [2u8; 16],
        direction: 0,
        offset: 0,
        price: 100.0 + order_id as f64,
        volume: 10.0,
        timestamp,
    }
}

/// 测试单条写入延迟分布
fn bench_single_write_latency() {
    println!("\n=== 单条写入延迟基准测试 ===");

    let tmp_dir = tempfile::tempdir().unwrap();
    let storage = OltpHybridStorage::create(
        "IF2501",
        OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 100 * 1024 * 1024, // 100MB，避免 flush
            estimated_entry_size: 256,
        },
    )
    .unwrap();

    // Warmup
    println!("热身中...");
    for i in 0..WARMUP_ITERATIONS {
        let record = create_test_record(i as u64, 1000 + i as i64);
        storage.write(record).unwrap();
    }

    // Benchmark
    println!("开始测试 {} 次写入...", BENCH_ITERATIONS);
    let mut latencies = Vec::with_capacity(BENCH_ITERATIONS);

    for i in 0..BENCH_ITERATIONS {
        let record = create_test_record((WARMUP_ITERATIONS + i) as u64, 2000 + i as i64);

        let start = Instant::now();
        storage.write(record).unwrap();
        let elapsed = start.elapsed();

        latencies.push(elapsed.as_micros());
    }

    // 统计
    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];
    let p999 = latencies[(latencies.len() as f64 * 0.999) as usize];
    let max = latencies[latencies.len() - 1];
    let avg = latencies.iter().sum::<u128>() / latencies.len() as u128;

    println!("\n写入延迟统计:");
    println!("  平均: {} μs", avg);
    println!("  P50:  {} μs", p50);
    println!("  P95:  {} μs", p95);
    println!("  P99:  {} μs", p99);
    println!("  P999: {} μs", p999);
    println!("  Max:  {} μs", max);

    println!("\n性能评估:");
    if p99 < 1_000 {
        println!("  ✓ 优秀: P99 < 1ms (SSD + 优化)");
    } else if p99 < 10_000 {
        println!("  ✓ 良好: P99 < 10ms (SSD)");
    } else if p99 < 50_000 {
        println!("  ○ 一般: P99 < 50ms (HDD/VM)");
    } else {
        println!("  ✗ 需优化: P99 > 50ms");
    }
}

/// 测试批量写入吞吐量
fn bench_batch_write_throughput() {
    println!("\n=== 批量写入吞吐量基准测试 ===");

    let tmp_dir = tempfile::tempdir().unwrap();
    let storage = OltpHybridStorage::create(
        "IF2501",
        OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 100 * 1024 * 1024,
            estimated_entry_size: 256,
        },
    )
    .unwrap();

    let batch_sizes = vec![100, 1000, 10000];

    for batch_size in batch_sizes {
        println!("\n测试批次大小: {}", batch_size);

        let start = Instant::now();
        for i in 0..batch_size {
            let record = create_test_record(i as u64, 1000 + i as i64);
            storage.write(record).unwrap();
        }
        let elapsed = start.elapsed();

        let throughput = batch_size as f64 / elapsed.as_secs_f64();
        let avg_latency = elapsed.as_micros() as f64 / batch_size as f64;

        println!("  耗时: {:?}", elapsed);
        println!("  吞吐量: {:.0} ops/s", throughput);
        println!("  平均延迟: {:.1} μs/op", avg_latency);
    }
}

/// 测试范围查询性能
fn bench_range_query() {
    println!("\n=== 范围查询性能基准测试 ===");

    let tmp_dir = tempfile::tempdir().unwrap();
    let storage = OltpHybridStorage::create(
        "IF2501",
        OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 10 * 1024 * 1024, // 10MB，会触发多次 flush
            estimated_entry_size: 256,
        },
    )
    .unwrap();

    // 写入 10000 条记录，会生成多个 SSTable
    println!("准备测试数据 (10000 条记录)...");
    for i in 0..10000 {
        let record = create_test_record(i as u64, 1000 + i as i64);
        storage.write(record).unwrap();
    }

    let stats = storage.stats();
    println!("数据准备完成:");
    println!("  MemTable 条目: {}", stats.memtable_entries);
    println!("  SSTable 数量: {}", stats.sstable_count);
    println!("  SSTable 条目: {}", stats.sstable_entries);

    // 测试不同范围的查询
    let query_ranges = vec![
        (1000, 1100, "小范围 (100 条)"),
        (1000, 2000, "中等范围 (1000 条)"),
        (1000, 6000, "大范围 (5000 条)"),
    ];

    for (start_ts, end_ts, desc) in query_ranges {
        let mut query_times = Vec::new();

        // 执行 10 次查询取平均
        for _ in 0..10 {
            let start = Instant::now();
            let results = storage.range_query(start_ts, end_ts).unwrap();
            let elapsed = start.elapsed();

            query_times.push(elapsed.as_micros());

            if query_times.len() == 1 {
                println!("\n{}:", desc);
                println!("  查询结果: {} 条", results.len());
            }
        }

        let avg = query_times.iter().sum::<u128>() / query_times.len() as u128;
        println!("  平均查询时间: {} μs", avg);
    }
}

/// 测试 Flush 性能
fn bench_flush_performance() {
    println!("\n=== Flush 性能基准测试 ===");

    let tmp_dir = tempfile::tempdir().unwrap();
    let storage = OltpHybridStorage::create(
        "IF2501",
        OltpHybridConfig {
            base_path: tmp_dir.path().to_str().unwrap().to_string(),
            memtable_size_bytes: 1 * 1024 * 1024, // 1MB，容易触发 flush
            estimated_entry_size: 256,
        },
    )
    .unwrap();

    println!("写入数据直到触发 5 次 flush...");

    let mut flush_count = 0;
    let mut total_writes = 0;
    let start = Instant::now();

    loop {
        let record = create_test_record(total_writes, 1000 + total_writes as i64);
        storage.write(record).unwrap();
        total_writes += 1;

        let stats = storage.stats();
        if stats.sstable_count > flush_count {
            flush_count = stats.sstable_count;
            println!(
                "  Flush #{}: {} entries → SSTable",
                flush_count, stats.sstable_entries
            );

            if flush_count >= 5 {
                break;
            }
        }
    }

    let elapsed = start.elapsed();
    println!("\n总计:");
    println!("  写入条目: {}", total_writes);
    println!("  触发 Flush: {} 次", flush_count);
    println!("  总耗时: {:?}", elapsed);
    println!(
        "  平均写入速度: {:.0} ops/s",
        total_writes as f64 / elapsed.as_secs_f64()
    );
}

/// 测试多品种并发写入
fn bench_concurrent_instruments() {
    println!("\n=== 多品种并发写入基准测试 ===");

    let tmp_dir = tempfile::tempdir().unwrap();
    let base_path = tmp_dir.path().to_str().unwrap().to_string();

    println!("启动 {} 个品种并发写入...", CONCURRENT_INSTRUMENTS);

    let start = Instant::now();
    let mut handles = Vec::new();

    for inst_id in 0..CONCURRENT_INSTRUMENTS {
        let base_path = base_path.clone();

        let handle = thread::spawn(move || {
            let instrument = format!("INST{:04}", inst_id);
            let storage = OltpHybridStorage::create(
                &instrument,
                OltpHybridConfig {
                    base_path,
                    memtable_size_bytes: 10 * 1024 * 1024,
                    estimated_entry_size: 256,
                },
            )
            .unwrap();

            // 每个品种写入 1000 条
            for i in 0..1000 {
                let record = create_test_record(i as u64, 1000 + i as i64);
                storage.write(record).unwrap();
            }

            storage.stats()
        });

        handles.push(handle);
    }

    // 等待所有线程完成
    let mut total_entries = 0;
    for handle in handles {
        let stats = handle.join().unwrap();
        total_entries += stats.memtable_entries + (stats.sstable_entries as usize);
    }

    let elapsed = start.elapsed();

    println!("\n并发测试结果:");
    println!("  品种数量: {}", CONCURRENT_INSTRUMENTS);
    println!("  总条目数: {}", total_entries);
    println!("  总耗时: {:?}", elapsed);
    println!(
        "  总吞吐量: {:.0} ops/s",
        total_entries as f64 / elapsed.as_secs_f64()
    );
    println!(
        "  单品种吞吐量: {:.0} ops/s",
        (total_entries / CONCURRENT_INSTRUMENTS) as f64 / elapsed.as_secs_f64()
    );
}

/// 测试崩溃恢复性能
fn bench_recovery_performance() {
    println!("\n=== 崩溃恢复性能基准测试 ===");

    let tmp_dir = tempfile::tempdir().unwrap();
    let base_path = tmp_dir.path().to_str().unwrap().to_string();

    let record_counts = vec![1000, 5000, 10000];

    for count in record_counts {
        println!("\n测试恢复 {} 条记录...", count);

        // 写入数据
        {
            let config = OltpHybridConfig {
                base_path: base_path.clone(),
                memtable_size_bytes: 100 * 1024 * 1024,
                estimated_entry_size: 256,
            };

            let storage = OltpHybridStorage::create("IF2501", config).unwrap();

            for i in 0..count {
                let record = create_test_record(i as u64, 1000 + i as i64);
                storage.write(record).unwrap();
            }

            println!("  数据写入完成");
        }

        // 恢复
        {
            let config = OltpHybridConfig {
                base_path: base_path.clone(),
                memtable_size_bytes: 100 * 1024 * 1024,
                estimated_entry_size: 256,
            };

            let storage = OltpHybridStorage::create("IF2501", config).unwrap();

            let start = Instant::now();
            storage.recover().unwrap();
            let elapsed = start.elapsed();

            let stats = storage.stats();

            println!("  恢复耗时: {:?}", elapsed);
            println!(
                "  恢复条目: {}",
                stats.memtable_entries + stats.sstable_entries as usize
            );
            println!(
                "  恢复速度: {:.0} entries/s",
                (stats.memtable_entries + stats.sstable_entries as usize) as f64
                    / elapsed.as_secs_f64()
            );
        }
    }
}

fn main() {
    println!("======================================");
    println!("   OLTP 存储性能基准测试套件");
    println!("======================================");

    bench_single_write_latency();
    bench_batch_write_throughput();
    bench_range_query();
    bench_flush_performance();
    bench_concurrent_instruments();
    bench_recovery_performance();

    println!("\n======================================");
    println!("   所有基准测试完成");
    println!("======================================");
}
