// Benchmark 测试：对比 serde JSON 和 rkyv 的序列化性能
//
// 运行方式：
// cargo bench --bench notification_serialization

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use qaexchange::notification::message::{
    Notification, NotificationType, NotificationPayload, AccountUpdateNotify,
};
use std::sync::Arc;

/// 创建测试通知
fn create_test_notification() -> Notification {
    let payload = NotificationPayload::AccountUpdate(AccountUpdateNotify {
        user_id: "user_benchmark_01".to_string(),
        balance: 1000000.0,
        available: 980000.0,
        frozen: 0.0,
        margin: 20000.0,
        position_profit: 500.0,
        close_profit: 1000.0,
        risk_ratio: 0.02,
        timestamp: 1728123456789,
    });

    Notification::new(
        NotificationType::AccountUpdate,
        Arc::from("user_benchmark_01"),
        payload,
        "AccountSystem",
    )
}

/// Benchmark: JSON 序列化（手动构造）
fn bench_json_manual_serialization(c: &mut Criterion) {
    let notification = create_test_notification();

    c.bench_function("json_manual_serialize", |b| {
        b.iter(|| {
            let _json = black_box(notification.to_json());
        });
    });
}

/// Benchmark: rkyv 序列化
fn bench_rkyv_serialization(c: &mut Criterion) {
    let notification = create_test_notification();

    c.bench_function("rkyv_serialize", |b| {
        b.iter(|| {
            let _bytes = black_box(notification.to_rkyv_bytes().unwrap());
        });
    });
}

/// Benchmark: rkyv 零拷贝反序列化
fn bench_rkyv_zero_copy_deserialize(c: &mut Criterion) {
    let notification = create_test_notification();
    let bytes = notification.to_rkyv_bytes().unwrap();

    c.bench_function("rkyv_zero_copy_deserialize", |b| {
        b.iter(|| {
            let _archived = black_box(Notification::from_rkyv_bytes(&bytes).unwrap());
        });
    });
}

/// Benchmark: rkyv 完整反序列化（包含内存分配）
fn bench_rkyv_full_deserialize(c: &mut Criterion) {
    let notification = create_test_notification();
    let bytes = notification.to_rkyv_bytes().unwrap();

    c.bench_function("rkyv_full_deserialize", |b| {
        b.iter(|| {
            let archived = Notification::from_rkyv_bytes(&bytes).unwrap();
            let _deserialized = black_box(Notification::from_archived(archived).unwrap());
        });
    });
}

/// Benchmark: 批量序列化（不同消息数量）
fn bench_batch_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_serialization");

    for count in [10, 100, 1000].iter() {
        // 创建批量通知
        let notifications: Vec<_> = (0..*count)
            .map(|_| create_test_notification())
            .collect();

        // JSON 手动序列化
        group.bench_with_input(
            BenchmarkId::new("json_manual", count),
            count,
            |b, _| {
                b.iter(|| {
                    let _jsons: Vec<_> = notifications
                        .iter()
                        .map(|n| black_box(n.to_json()))
                        .collect();
                });
            },
        );

        // rkyv 序列化
        group.bench_with_input(
            BenchmarkId::new("rkyv_serialize", count),
            count,
            |b, _| {
                b.iter(|| {
                    let _bytes: Vec<_> = notifications
                        .iter()
                        .map(|n| black_box(n.to_rkyv_bytes().unwrap()))
                        .collect();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: 批量反序列化（不同消息数量）
fn bench_batch_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_deserialization");

    for count in [10, 100, 1000].iter() {
        // 创建并序列化批量通知
        let notifications: Vec<_> = (0..*count)
            .map(|_| create_test_notification())
            .collect();

        let serialized: Vec<_> = notifications
            .iter()
            .map(|n| n.to_rkyv_bytes().unwrap())
            .collect();

        // rkyv 零拷贝反序列化
        group.bench_with_input(
            BenchmarkId::new("rkyv_zero_copy", count),
            count,
            |b, _| {
                b.iter(|| {
                    let _archived: Vec<_> = serialized
                        .iter()
                        .map(|bytes| black_box(Notification::from_rkyv_bytes(bytes).unwrap()))
                        .collect();
                });
            },
        );

        // rkyv 完整反序列化
        group.bench_with_input(
            BenchmarkId::new("rkyv_full", count),
            count,
            |b, _| {
                b.iter(|| {
                    let _deserialized: Vec<_> = serialized
                        .iter()
                        .map(|bytes| {
                            let archived = Notification::from_rkyv_bytes(bytes).unwrap();
                            black_box(Notification::from_archived(archived).unwrap())
                        })
                        .collect();
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_json_manual_serialization,
    bench_rkyv_serialization,
    bench_rkyv_zero_copy_deserialize,
    bench_rkyv_full_deserialize,
    bench_batch_serialization,
    bench_batch_deserialization,
);

criterion_main!(benches);
