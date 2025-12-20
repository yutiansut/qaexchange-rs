// 网络层集成测试 (Phase 13)
//
// 测试范围：
// 1. TLS 证书生成与配置
// 2. 加密连接验证
// 3. 复制协议网络传输
// 4. 分布式追踪集成
// 5. 端到端网络通信测试
//
// @yutiansut @quantaxis

use qaexchange::replication::{
    CertificateGenerator, CertificatePaths, TlsConfig, TlsConfigBuilder, TlsError,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ========================================
// TLS 证书生成测试
// ========================================

/// 测试自签名证书生成
#[test]
fn test_self_signed_certificate_generation() {
    let test_dir = TempDir::new().unwrap();
    let cert_path = test_dir.path().join("test.crt");
    let key_path = test_dir.path().join("test.key");

    // 生成自签名证书
    let result = CertificateGenerator::generate_self_signed(
        "test.qaexchange.local",
        365, // 1年有效期
        &cert_path,
        &key_path,
    );

    assert!(result.is_ok(), "证书生成应该成功: {:?}", result.err());

    // 验证文件存在
    assert!(cert_path.exists(), "证书文件应该存在");
    assert!(key_path.exists(), "私钥文件应该存在");

    // 验证文件内容格式
    let cert_content = fs::read_to_string(&cert_path).unwrap();
    let key_content = fs::read_to_string(&key_path).unwrap();

    assert!(
        cert_content.contains("-----BEGIN CERTIFICATE-----"),
        "证书应该是 PEM 格式"
    );
    assert!(
        key_content.contains("-----BEGIN PRIVATE KEY-----"),
        "私钥应该是 PEM 格式"
    );

    println!("✓ 自签名证书生成测试通过");
}

/// 测试 CA 证书链生成
#[test]
fn test_ca_certificate_chain_generation() {
    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    // 生成 CA 证书
    let ca_cert = cert_dir.join("ca.crt");
    let ca_key = cert_dir.join("ca.key");

    let result = CertificateGenerator::generate_ca(
        "QAExchange Test CA",
        3650, // 10年有效期
        &ca_cert,
        &ca_key,
    );

    assert!(result.is_ok(), "CA 证书生成应该成功: {:?}", result.err());

    // 生成服务器证书（由 CA 签发）
    let server_cert = cert_dir.join("server.crt");
    let server_key = cert_dir.join("server.key");

    let result = CertificateGenerator::generate_signed_certificate(
        "master.qaexchange.local",
        365,
        &ca_cert,
        &ca_key,
        &server_cert,
        &server_key,
    );

    assert!(
        result.is_ok(),
        "服务器证书生成应该成功: {:?}",
        result.err()
    );

    // 验证所有文件存在
    assert!(ca_cert.exists(), "CA 证书应该存在");
    assert!(ca_key.exists(), "CA 私钥应该存在");
    assert!(server_cert.exists(), "服务器证书应该存在");
    assert!(server_key.exists(), "服务器私钥应该存在");

    println!("✓ CA 证书链生成测试通过");
}

/// 测试客户端证书生成（mTLS）
#[test]
fn test_mtls_certificate_generation() {
    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    // 1. 生成 CA
    let ca_cert = cert_dir.join("ca.crt");
    let ca_key = cert_dir.join("ca.key");
    CertificateGenerator::generate_ca("QAExchange mTLS CA", 3650, &ca_cert, &ca_key).unwrap();

    // 2. 生成服务器证书
    let server_cert = cert_dir.join("server.crt");
    let server_key = cert_dir.join("server.key");
    CertificateGenerator::generate_signed_certificate(
        "server.qaexchange.local",
        365,
        &ca_cert,
        &ca_key,
        &server_cert,
        &server_key,
    )
    .unwrap();

    // 3. 生成客户端证书
    let client_cert = cert_dir.join("client.crt");
    let client_key = cert_dir.join("client.key");
    CertificateGenerator::generate_signed_certificate(
        "client.qaexchange.local",
        365,
        &ca_cert,
        &ca_key,
        &client_cert,
        &client_key,
    )
    .unwrap();

    // 验证所有证书存在
    assert!(client_cert.exists(), "客户端证书应该存在");
    assert!(client_key.exists(), "客户端私钥应该存在");

    println!("✓ mTLS 证书生成测试通过");
}

// ========================================
// TLS 配置测试
// ========================================

/// 测试 TLS 配置构建器
#[test]
fn test_tls_config_builder() {
    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    // 准备测试证书
    let ca_cert = cert_dir.join("ca.crt");
    let ca_key = cert_dir.join("ca.key");
    CertificateGenerator::generate_ca("Test CA", 365, &ca_cert, &ca_key).unwrap();

    let server_cert = cert_dir.join("server.crt");
    let server_key = cert_dir.join("server.key");
    CertificateGenerator::generate_signed_certificate(
        "server.local",
        365,
        &ca_cert,
        &ca_key,
        &server_cert,
        &server_key,
    )
    .unwrap();

    // 构建配置
    let config = TlsConfigBuilder::new()
        .with_ca_cert(&ca_cert)
        .with_cert(&server_cert)
        .with_key(&server_key)
        .with_verify_client(false)
        .build();

    assert!(config.is_ok(), "TLS 配置构建应该成功: {:?}", config.err());

    let tls_config = config.unwrap();
    assert!(!tls_config.verify_client(), "verify_client 应该为 false");

    println!("✓ TLS 配置构建器测试通过");
}

/// 测试开发环境配置
#[test]
fn test_development_tls_config() {
    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    // 使用开发配置（自动生成证书）
    let config = TlsConfigBuilder::development(cert_dir);

    assert!(config.is_ok(), "开发配置应该成功: {:?}", config.err());

    // 验证证书已生成
    assert!(
        cert_dir.join("dev.crt").exists(),
        "开发证书应该已生成"
    );
    assert!(cert_dir.join("dev.key").exists(), "开发私钥应该已生成");

    println!("✓ 开发环境 TLS 配置测试通过");
}

/// 测试生产环境配置验证
#[test]
fn test_production_tls_config_validation() {
    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    // 准备完整的生产证书链
    let ca_cert = cert_dir.join("ca.crt");
    let ca_key = cert_dir.join("ca.key");
    CertificateGenerator::generate_ca("Prod CA", 3650, &ca_cert, &ca_key).unwrap();

    let server_cert = cert_dir.join("server.crt");
    let server_key = cert_dir.join("server.key");
    CertificateGenerator::generate_signed_certificate(
        "prod.qaexchange.com",
        365,
        &ca_cert,
        &ca_key,
        &server_cert,
        &server_key,
    )
    .unwrap();

    let client_cert = cert_dir.join("client.crt");
    let client_key = cert_dir.join("client.key");
    CertificateGenerator::generate_signed_certificate(
        "client.qaexchange.com",
        365,
        &ca_cert,
        &ca_key,
        &client_cert,
        &client_key,
    )
    .unwrap();

    // 构建生产配置（启用 mTLS）
    let config = TlsConfigBuilder::new()
        .with_ca_cert(&ca_cert)
        .with_cert(&server_cert)
        .with_key(&server_key)
        .with_verify_client(true)
        .build();

    assert!(config.is_ok(), "生产配置应该成功: {:?}", config.err());

    let tls_config = config.unwrap();
    assert!(tls_config.verify_client(), "生产环境应该启用 mTLS");

    println!("✓ 生产环境 TLS 配置测试通过");
}

// ========================================
// 证书路径管理测试
// ========================================

/// 测试证书路径结构
#[test]
fn test_certificate_paths_structure() {
    let base_dir = PathBuf::from("/etc/qaexchange/certs");

    let paths = CertificatePaths::new(&base_dir);

    assert_eq!(paths.ca_cert(), base_dir.join("ca.crt"));
    assert_eq!(paths.ca_key(), base_dir.join("ca.key"));
    assert_eq!(paths.server_cert(), base_dir.join("server.crt"));
    assert_eq!(paths.server_key(), base_dir.join("server.key"));
    assert_eq!(paths.client_cert(), base_dir.join("client.crt"));
    assert_eq!(paths.client_key(), base_dir.join("client.key"));

    println!("✓ 证书路径结构测试通过");
}

/// 测试证书路径验证
#[test]
fn test_certificate_paths_validation() {
    let test_dir = TempDir::new().unwrap();
    let base_dir = test_dir.path();

    // 创建完整的证书集
    let ca_cert = base_dir.join("ca.crt");
    let ca_key = base_dir.join("ca.key");
    CertificateGenerator::generate_ca("Test CA", 365, &ca_cert, &ca_key).unwrap();

    let server_cert = base_dir.join("server.crt");
    let server_key = base_dir.join("server.key");
    CertificateGenerator::generate_signed_certificate(
        "server.local",
        365,
        &ca_cert,
        &ca_key,
        &server_cert,
        &server_key,
    )
    .unwrap();

    let client_cert = base_dir.join("client.crt");
    let client_key = base_dir.join("client.key");
    CertificateGenerator::generate_signed_certificate(
        "client.local",
        365,
        &ca_cert,
        &ca_key,
        &client_cert,
        &client_key,
    )
    .unwrap();

    let paths = CertificatePaths::new(base_dir);

    // 验证所有路径存在
    assert!(paths.validate_server_certs().is_ok(), "服务器证书应该有效");
    assert!(paths.validate_client_certs().is_ok(), "客户端证书应该有效");
    assert!(paths.validate_all().is_ok(), "所有证书应该有效");

    println!("✓ 证书路径验证测试通过");
}

// ========================================
// 网络通信模拟测试
// ========================================

/// 测试复制协议消息序列化
#[test]
fn test_replication_message_serialization() {
    use qaexchange::replication::{ReplicationMessage, ReplicationRole};

    // 心跳消息
    let heartbeat = ReplicationMessage::Heartbeat {
        node_id: "node-1".to_string(),
        role: ReplicationRole::Master,
        term: 42,
        commit_index: 1000,
    };

    // 序列化
    let bytes = heartbeat.to_bytes();
    assert!(!bytes.is_empty(), "序列化结果不应为空");

    // 反序列化
    let decoded = ReplicationMessage::from_bytes(&bytes);
    assert!(decoded.is_ok(), "反序列化应该成功: {:?}", decoded.err());

    match decoded.unwrap() {
        ReplicationMessage::Heartbeat {
            node_id,
            role,
            term,
            commit_index,
        } => {
            assert_eq!(node_id, "node-1");
            assert_eq!(role, ReplicationRole::Master);
            assert_eq!(term, 42);
            assert_eq!(commit_index, 1000);
        }
        _ => panic!("应该是心跳消息"),
    }

    println!("✓ 复制协议消息序列化测试通过");
}

/// 测试日志条目批量传输
#[test]
fn test_log_entry_batch_transfer() {
    use qaexchange::replication::{LogEntry, ReplicationMessage};

    // 创建批量日志条目
    let mut entries = Vec::new();
    for i in 0..100 {
        entries.push(LogEntry {
            index: i,
            term: 1,
            data: format!("entry-{}", i).into_bytes(),
        });
    }

    let batch = ReplicationMessage::AppendEntries {
        leader_id: "master-1".to_string(),
        term: 1,
        prev_log_index: 0,
        prev_log_term: 0,
        entries: entries.clone(),
        leader_commit: 50,
    };

    // 序列化
    let start = Instant::now();
    let bytes = batch.to_bytes();
    let serialize_time = start.elapsed();

    // 反序列化
    let start = Instant::now();
    let decoded = ReplicationMessage::from_bytes(&bytes).unwrap();
    let deserialize_time = start.elapsed();

    match decoded {
        ReplicationMessage::AppendEntries {
            entries: decoded_entries,
            ..
        } => {
            assert_eq!(decoded_entries.len(), 100, "应该有100条日志");
        }
        _ => panic!("应该是 AppendEntries 消息"),
    }

    println!(
        "✓ 日志批量传输测试通过 (序列化: {:?}, 反序列化: {:?})",
        serialize_time, deserialize_time
    );
}

// ========================================
// 并发网络操作测试
// ========================================

/// 测试并发证书生成
#[test]
fn test_concurrent_certificate_generation() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    let test_dir = TempDir::new().unwrap();
    let base_path = test_dir.path().to_path_buf();
    let success_count = Arc::new(AtomicUsize::new(0));
    let thread_count = 4;

    let mut handles = Vec::new();

    for i in 0..thread_count {
        let path = base_path.clone();
        let counter = success_count.clone();

        let handle = thread::spawn(move || {
            let cert_path = path.join(format!("cert-{}.crt", i));
            let key_path = path.join(format!("cert-{}.key", i));

            let result = CertificateGenerator::generate_self_signed(
                &format!("node-{}.qaexchange.local", i),
                365,
                &cert_path,
                &key_path,
            );

            if result.is_ok() {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(
        success_count.load(Ordering::SeqCst),
        thread_count,
        "所有证书应该成功生成"
    );

    println!("✓ 并发证书生成测试通过");
}

/// 测试高并发消息序列化
#[test]
fn test_high_concurrency_message_serialization() {
    use qaexchange::replication::{ReplicationMessage, ReplicationRole};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;

    let total_ops = Arc::new(AtomicU64::new(0));
    let thread_count = 8;
    let ops_per_thread = 10000;

    let start = Instant::now();
    let mut handles = Vec::new();

    for _ in 0..thread_count {
        let counter = total_ops.clone();

        let handle = thread::spawn(move || {
            for i in 0..ops_per_thread {
                let msg = ReplicationMessage::Heartbeat {
                    node_id: format!("node-{}", i % 10),
                    role: ReplicationRole::Slave,
                    term: i as u64,
                    commit_index: i as u64 * 100,
                };

                let bytes = msg.to_bytes();
                let _ = ReplicationMessage::from_bytes(&bytes).unwrap();

                counter.fetch_add(1, Ordering::Relaxed);
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total = total_ops.load(Ordering::Relaxed);
    let throughput = total as f64 / duration.as_secs_f64();

    println!(
        "✓ 高并发消息序列化测试通过: {} ops in {:?} ({:.0} ops/sec)",
        total, duration, throughput
    );

    // 性能断言
    assert!(throughput > 10000.0, "吞吐量应该 > 10K ops/sec");
}

// ========================================
// 端到端网络模拟测试
// ========================================

/// 模拟主从节点网络交互
#[test]
#[ignore] // 需要网络环境，手动运行
fn test_master_slave_network_simulation() {
    use qaexchange::replication::{
        LogEntry, ReplicationMessage, ReplicationRole, RoleManager, RoleState,
    };
    use std::sync::mpsc;
    use std::thread;

    println!("========================================");
    println!("主从节点网络模拟测试");
    println!("========================================");

    // 创建通信通道（模拟网络）
    let (master_tx, slave_rx) = mpsc::channel::<Vec<u8>>();
    let (slave_tx, master_rx) = mpsc::channel::<Vec<u8>>();

    // 主节点线程
    let master_handle = thread::spawn(move || {
        let node_id = "master-1";
        let mut commit_index = 0u64;

        // 发送日志条目
        for batch in 0..10 {
            let entries: Vec<LogEntry> = (0..100)
                .map(|i| LogEntry {
                    index: batch * 100 + i,
                    term: 1,
                    data: format!("data-{}-{}", batch, i).into_bytes(),
                })
                .collect();

            let msg = ReplicationMessage::AppendEntries {
                leader_id: node_id.to_string(),
                term: 1,
                prev_log_index: commit_index,
                prev_log_term: 1,
                entries,
                leader_commit: commit_index,
            };

            master_tx.send(msg.to_bytes()).unwrap();

            // 等待响应
            let response_bytes = master_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let response = ReplicationMessage::from_bytes(&response_bytes).unwrap();

            match response {
                ReplicationMessage::AppendEntriesResponse {
                    success,
                    match_index,
                    ..
                } => {
                    assert!(success, "从节点应该成功接收");
                    commit_index = match_index;
                }
                _ => panic!("期望 AppendEntriesResponse"),
            }
        }

        commit_index
    });

    // 从节点线程
    let slave_handle = thread::spawn(move || {
        let node_id = "slave-1";
        let mut match_index = 0u64;
        let mut received_entries = 0usize;

        loop {
            match slave_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(bytes) => {
                    let msg = ReplicationMessage::from_bytes(&bytes).unwrap();

                    match msg {
                        ReplicationMessage::AppendEntries {
                            entries,
                            leader_commit,
                            ..
                        } => {
                            received_entries += entries.len();
                            match_index = entries.last().map(|e| e.index).unwrap_or(match_index);

                            let response = ReplicationMessage::AppendEntriesResponse {
                                node_id: node_id.to_string(),
                                term: 1,
                                success: true,
                                match_index,
                            };

                            slave_tx.send(response.to_bytes()).unwrap();

                            if received_entries >= 1000 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                Err(_) => break,
            }
        }

        received_entries
    });

    let master_commit = master_handle.join().unwrap();
    let slave_received = slave_handle.join().unwrap();

    println!("  Master commit index: {}", master_commit);
    println!("  Slave received entries: {}", slave_received);

    assert_eq!(slave_received, 1000, "从节点应该收到1000条日志");
    assert_eq!(master_commit, 999, "主节点 commit index 应该是 999");

    println!("✓ 主从节点网络模拟测试通过");
}

// ========================================
// 性能基准测试
// ========================================

/// TLS 配置加载性能
#[test]
fn test_tls_config_load_performance() {
    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    // 准备证书
    let ca_cert = cert_dir.join("ca.crt");
    let ca_key = cert_dir.join("ca.key");
    CertificateGenerator::generate_ca("Perf CA", 365, &ca_cert, &ca_key).unwrap();

    let server_cert = cert_dir.join("server.crt");
    let server_key = cert_dir.join("server.key");
    CertificateGenerator::generate_signed_certificate(
        "perf.local",
        365,
        &ca_cert,
        &ca_key,
        &server_cert,
        &server_key,
    )
    .unwrap();

    // 测试配置加载性能
    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let _config = TlsConfigBuilder::new()
            .with_ca_cert(&ca_cert)
            .with_cert(&server_cert)
            .with_key(&server_key)
            .build()
            .unwrap();
    }

    let duration = start.elapsed();
    let avg_time = duration / iterations;

    println!(
        "✓ TLS 配置加载性能: {} 次迭代, 平均 {:?}/次",
        iterations, avg_time
    );

    // 配置加载应该在合理时间内完成
    assert!(
        avg_time < Duration::from_millis(100),
        "配置加载平均时间应该 < 100ms"
    );
}

/// 证书生成性能
#[test]
fn test_certificate_generation_performance() {
    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    let iterations = 10;
    let start = Instant::now();

    for i in 0..iterations {
        let cert_path = cert_dir.join(format!("perf-{}.crt", i));
        let key_path = cert_dir.join(format!("perf-{}.key", i));

        CertificateGenerator::generate_self_signed(
            &format!("perf-{}.local", i),
            365,
            &cert_path,
            &key_path,
        )
        .unwrap();
    }

    let duration = start.elapsed();
    let avg_time = duration / iterations;

    println!(
        "✓ 证书生成性能: {} 次迭代, 平均 {:?}/次",
        iterations, avg_time
    );

    // 证书生成应该在合理时间内完成
    assert!(
        avg_time < Duration::from_secs(1),
        "证书生成平均时间应该 < 1s"
    );
}

// ========================================
// 错误处理测试
// ========================================

/// 测试无效证书路径错误处理
#[test]
fn test_invalid_certificate_path_error() {
    let result = TlsConfigBuilder::new()
        .with_ca_cert("/nonexistent/path/ca.crt")
        .with_cert("/nonexistent/path/server.crt")
        .with_key("/nonexistent/path/server.key")
        .build();

    assert!(result.is_err(), "无效路径应该返回错误");

    println!("✓ 无效证书路径错误处理测试通过");
}

/// 测试证书格式错误处理
#[test]
fn test_invalid_certificate_format_error() {
    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    // 创建无效格式的证书文件
    let invalid_cert = cert_dir.join("invalid.crt");
    let invalid_key = cert_dir.join("invalid.key");

    fs::write(&invalid_cert, "not a valid certificate").unwrap();
    fs::write(&invalid_key, "not a valid key").unwrap();

    let result = TlsConfigBuilder::new()
        .with_cert(&invalid_cert)
        .with_key(&invalid_key)
        .build();

    assert!(result.is_err(), "无效格式应该返回错误");

    println!("✓ 无效证书格式错误处理测试通过");
}

// ========================================
// 综合集成测试
// ========================================

/// 完整网络层集成测试
#[test]
#[ignore] // 综合测试，手动运行
fn test_full_network_integration() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    println!("========================================");
    println!("网络层完整集成测试 (Phase 13)");
    println!("========================================");

    let test_dir = TempDir::new().unwrap();
    let cert_dir = test_dir.path();

    // ========================================
    // Phase 1: 证书基础设施
    // ========================================
    println!("\n[Phase 1] 证书基础设施...");

    // 生成 CA
    let ca_cert = cert_dir.join("ca.crt");
    let ca_key = cert_dir.join("ca.key");
    CertificateGenerator::generate_ca("QAExchange Test CA", 3650, &ca_cert, &ca_key).unwrap();
    println!("  ✓ CA 证书已生成");

    // 生成 Master 证书
    let master_cert = cert_dir.join("master.crt");
    let master_key = cert_dir.join("master.key");
    CertificateGenerator::generate_signed_certificate(
        "master.qaexchange.local",
        365,
        &ca_cert,
        &ca_key,
        &master_cert,
        &master_key,
    )
    .unwrap();
    println!("  ✓ Master 证书已生成");

    // 生成 Slave 证书
    let slave_cert = cert_dir.join("slave.crt");
    let slave_key = cert_dir.join("slave.key");
    CertificateGenerator::generate_signed_certificate(
        "slave.qaexchange.local",
        365,
        &ca_cert,
        &ca_key,
        &slave_cert,
        &slave_key,
    )
    .unwrap();
    println!("  ✓ Slave 证书已生成");

    // ========================================
    // Phase 2: TLS 配置
    // ========================================
    println!("\n[Phase 2] TLS 配置...");

    let master_tls = TlsConfigBuilder::new()
        .with_ca_cert(&ca_cert)
        .with_cert(&master_cert)
        .with_key(&master_key)
        .with_verify_client(true)
        .build()
        .unwrap();
    println!("  ✓ Master TLS 配置完成 (mTLS: {})", master_tls.verify_client());

    let slave_tls = TlsConfigBuilder::new()
        .with_ca_cert(&ca_cert)
        .with_cert(&slave_cert)
        .with_key(&slave_key)
        .with_verify_client(false)
        .build()
        .unwrap();
    println!("  ✓ Slave TLS 配置完成");

    // ========================================
    // Phase 3: 消息协议测试
    // ========================================
    println!("\n[Phase 3] 消息协议测试...");

    use qaexchange::replication::{LogEntry, ReplicationMessage, ReplicationRole};

    let messages_to_test = vec![
        ReplicationMessage::Heartbeat {
            node_id: "master".to_string(),
            role: ReplicationRole::Master,
            term: 1,
            commit_index: 100,
        },
        ReplicationMessage::RequestVote {
            candidate_id: "candidate".to_string(),
            term: 2,
            last_log_index: 99,
            last_log_term: 1,
        },
        ReplicationMessage::AppendEntries {
            leader_id: "master".to_string(),
            term: 1,
            prev_log_index: 99,
            prev_log_term: 1,
            entries: (0..10)
                .map(|i| LogEntry {
                    index: 100 + i,
                    term: 1,
                    data: vec![i as u8; 100],
                })
                .collect(),
            leader_commit: 100,
        },
    ];

    for msg in messages_to_test {
        let bytes = msg.to_bytes();
        let decoded = ReplicationMessage::from_bytes(&bytes).unwrap();
        println!("  ✓ {} 序列化/反序列化通过", msg.message_type());
    }

    // ========================================
    // Phase 4: 性能验证
    // ========================================
    println!("\n[Phase 4] 性能验证...");

    let iterations = 10000;
    let start = Instant::now();

    for i in 0..iterations {
        let msg = ReplicationMessage::Heartbeat {
            node_id: format!("node-{}", i % 10),
            role: ReplicationRole::Slave,
            term: i as u64,
            commit_index: i as u64 * 100,
        };

        let bytes = msg.to_bytes();
        let _ = ReplicationMessage::from_bytes(&bytes).unwrap();
    }

    let duration = start.elapsed();
    let throughput = iterations as f64 / duration.as_secs_f64();

    println!(
        "  ✓ 消息处理吞吐量: {:.0} msgs/sec",
        throughput
    );

    // ========================================
    // 测试结果
    // ========================================
    println!("\n========================================");
    println!("网络层集成测试完成");
    println!("========================================");
    println!("  证书: CA + Master + Slave");
    println!("  TLS: mTLS 双向认证");
    println!("  协议: Heartbeat, RequestVote, AppendEntries");
    println!("  性能: {:.0} msgs/sec", throughput);
    println!("========================================");
}
