# 测试指南

**版本**: v0.1.0
**更新日期**: 2025-10-03
**开发团队**: @yutiansut

---

## 📋 目录

1. [测试概览](#测试概览)
2. [单元测试](#单元测试)
3. [集成测试](#集成测试)
4. [性能测试](#性能测试)
5. [端到端测试](#端到端测试)
6. [测试覆盖率](#测试覆盖率)
7. [最佳实践](#最佳实践)

---

## 测试概览

### 测试金字塔

```
           ┌───────────────┐
           │  E2E 测试     │  ← 少量，覆盖关键流程
           │  (5%)         │
           ├───────────────┤
           │  集成测试     │  ← 中量，测试模块交互
           │  (15%)        │
           ├───────────────┤
           │  单元测试     │  ← 大量，测试单个函数
           │  (80%)        │
           └───────────────┘
```

### 测试策略

| 测试类型 | 覆盖范围 | 执行速度 | 数量占比 |
|---------|---------|---------|---------|
| 单元测试 | 单个函数/方法 | 极快 (< 1ms) | 80% |
| 集成测试 | 多个模块交互 | 快 (< 100ms) | 15% |
| E2E 测试 | 完整业务流程 | 慢 (> 1s) | 5% |

### 当前测试状态

```bash
cargo test --lib
```

**结果**:
```
running 31 tests
test result: ok. 31 passed; 0 failed; 0 ignored
```

**测试分布**:
- PreTradeCheck: 4 tests
- OrderRouter: 6 tests
- TradeGateway: 5 tests
- SettlementEngine: 2 tests
- AccountManager (from qars): 14 tests

---

## 单元测试

### 编写单元测试

**基本结构**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        // Arrange: 准备测试数据
        let input = 10;

        // Act: 执行被测试代码
        let result = function_under_test(input);

        // Assert: 验证结果
        assert_eq!(result, expected_value);
    }
}
```

### PreTradeCheck 测试示例

**测试风控拒绝**:
```rust
// src/risk/pre_trade_check.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_env() -> (Arc<AccountManager>, PreTradeCheck) {
        let account_mgr = Arc::new(AccountManager::new());

        // 创建测试账户
        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            user_name: "Test User".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
            password: "test123".to_string(),
        };
        account_mgr.open_account(req).unwrap();

        let checker = PreTradeCheck::new(account_mgr.clone());
        (account_mgr, checker)
    }

    #[test]
    fn test_insufficient_funds() {
        let (_, checker) = create_test_env();

        // 超出资金的订单
        let req = OrderCheckRequest {
            user_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 1000.0,  // 需要 1000 * 120 * 10% = 12000 保证金
            price: 120.0,
        };

        match checker.check(&req).unwrap() {
            RiskCheckResult::Reject { reason, code } => {
                assert_eq!(code, RiskCheckCode::InsufficientFunds);
                assert!(reason.contains("资金不足"));
            }
            RiskCheckResult::Pass => panic!("Expected rejection"),
        }
    }

    #[test]
    fn test_valid_order() {
        let (_, checker) = create_test_env();

        // 合理的订单
        let req = OrderCheckRequest {
            user_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,  // 需要 10 * 120 * 10% = 120 保证金
            price: 120.0,
        };

        assert!(matches!(
            checker.check(&req).unwrap(),
            RiskCheckResult::Pass
        ));
    }
}
```

### OrderRouter 测试示例

**测试订单提交流程**:
```rust
// src/exchange/order_router.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_router() -> OrderRouter {
        let account_mgr = Arc::new(AccountManager::new());
        let risk_checker = Arc::new(PreTradeCheck::new(account_mgr.clone()));
        let trade_gateway = Arc::new(TradeGateway::new(account_mgr.clone()));

        let mut router = OrderRouter::new(account_mgr, risk_checker, trade_gateway);

        // 注册合约
        router.register_instrument("IX2301", 1.0, 0.0001);

        router
    }

    #[test]
    fn test_submit_order_success() {
        let router = create_test_router();

        // 创建账户
        router.account_mgr.open_account(OpenAccountRequest {
            user_id: "user001".to_string(),
            user_name: "User 1".to_string(),
            init_cash: 1000000.0,
            account_type: AccountType::Individual,
            password: "pass".to_string(),
        }).unwrap();

        // 提交订单
        let req = SubmitOrderRequest {
            user_id: "user001".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            order_type: "LIMIT".to_string(),
        };

        let response = router.submit_order(req);

        assert!(response.success);
        assert!(response.order_id.is_some());
        assert!(response.error_message.is_none());
    }

    #[test]
    fn test_cancel_order() {
        let router = create_test_router();

        // 提交订单
        // ...

        // 撤销订单
        let result = router.cancel_order("O12345");
        assert!(result.is_ok());
    }
}
```

### 断言宏

```rust
// 相等断言
assert_eq!(actual, expected);
assert_ne!(actual, unexpected);

// 布尔断言
assert!(condition);
assert!(!condition);

// 模式匹配断言
assert!(matches!(result, RiskCheckResult::Pass));

// 浮点数断言 (考虑精度)
fn assert_float_eq(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6);
}

// 自定义错误消息
assert_eq!(actual, expected, "Expected {} but got {}", expected, actual);
```

---

## 集成测试

### 创建集成测试

**目录结构**:
```
tests/
├── common/
│   └── mod.rs         # 共享测试工具
├── test_order_flow.rs # 订单流程测试
└── test_settlement.rs # 结算流程测试
```

### 完整订单流程测试

**tests/test_order_flow.rs**:
```rust
use qaexchange::exchange::{OrderRouter, TradeGateway};
use qaexchange::core::{AccountManager, OpenAccountRequest, AccountType};
use qaexchange::risk::PreTradeCheck;
use std::sync::Arc;

#[test]
fn test_full_order_lifecycle() {
    // 1. 创建系统组件
    let account_mgr = Arc::new(AccountManager::new());
    let risk_checker = Arc::new(PreTradeCheck::new(account_mgr.clone()));
    let trade_gateway = Arc::new(TradeGateway::new(account_mgr.clone()));
    let mut router = OrderRouter::new(
        account_mgr.clone(),
        risk_checker,
        trade_gateway.clone()
    );

    // 2. 注册合约
    router.register_instrument("IX2301", 1.0, 0.0001);

    // 3. 开户
    account_mgr.open_account(OpenAccountRequest {
        user_id: "buyer".to_string(),
        user_name: "Buyer".to_string(),
        init_cash: 1000000.0,
        account_type: AccountType::Individual,
        password: "pass".to_string(),
    }).unwrap();

    account_mgr.open_account(OpenAccountRequest {
        user_id: "seller".to_string(),
        user_name: "Seller".to_string(),
        init_cash: 1000000.0,
        account_type: AccountType::Individual,
        password: "pass".to_string(),
    }).unwrap();

    // 4. 提交买单
    let buy_req = SubmitOrderRequest {
        user_id: "buyer".to_string(),
        instrument_id: "IX2301".to_string(),
        direction: "BUY".to_string(),
        offset: "OPEN".to_string(),
        volume: 10.0,
        price: 120.0,
        order_type: "LIMIT".to_string(),
    };
    let buy_response = router.submit_order(buy_req);
    assert!(buy_response.success);

    // 5. 提交卖单 (价格匹配，应成交)
    let sell_req = SubmitOrderRequest {
        user_id: "seller".to_string(),
        instrument_id: "IX2301".to_string(),
        direction: "SELL".to_string(),
        offset: "OPEN".to_string(),
        volume: 10.0,
        price: 120.0,
        order_type: "LIMIT".to_string(),
    };
    let sell_response = router.submit_order(sell_req);
    assert!(sell_response.success);

    // 6. 验证账户状态
    let buyer_account = account_mgr.get_account("buyer").unwrap();
    let buyer_acc = buyer_account.read();
    assert_eq!(buyer_acc.hold.get("IX2301").unwrap().volume_long_today, 10.0);

    let seller_account = account_mgr.get_account("seller").unwrap();
    let seller_acc = seller_account.read();
    assert_eq!(seller_acc.hold.get("IX2301").unwrap().volume_short_today, 10.0);

    // 7. 平仓
    let close_req = SubmitOrderRequest {
        user_id: "buyer".to_string(),
        instrument_id: "IX2301".to_string(),
        direction: "SELL".to_string(),
        offset: "CLOSETODAY".to_string(),
        volume: 10.0,
        price: 125.0,  // 盈利平仓
        order_type: "LIMIT".to_string(),
    };
    let close_response = router.submit_order(close_req);
    assert!(close_response.success);

    // 8. 验证盈亏
    let buyer_acc = buyer_account.read();
    assert!(buyer_acc.accounts.close_profit > 0.0);  // 有平仓盈利
}
```

### 异步测试

```rust
#[tokio::test]
async fn test_async_operation() {
    let result = some_async_function().await;
    assert!(result.is_ok());
}
```

---

## 性能测试

### Criterion 基准测试

**安装 Criterion**:
```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "order_router"
harness = false
```

**编写基准测试**:
```rust
// benches/order_router.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use qaexchange::exchange::OrderRouter;

fn benchmark_submit_order(c: &mut Criterion) {
    let router = create_test_router();

    c.bench_function("submit_order", |b| {
        b.iter(|| {
            let req = create_test_request();
            router.submit_order(black_box(req))
        });
    });
}

criterion_group!(benches, benchmark_submit_order);
criterion_main!(benches);
```

**运行基准测试**:
```bash
cargo bench

# 查看报告
open target/criterion/submit_order/report/index.html
```

### 压力测试

**并发订单提交**:
```rust
#[test]
fn test_concurrent_orders() {
    use std::thread;

    let router = Arc::new(create_test_router());
    let mut handles = vec![];

    // 启动 100 个线程同时提交订单
    for i in 0..100 {
        let router_clone = router.clone();
        let handle = thread::spawn(move || {
            let req = create_test_request_for_user(&format!("user{}", i));
            router_clone.submit_order(req)
        });
        handles.push(handle);
    }

    // 等待所有线程完成
    for handle in handles {
        let response = handle.join().unwrap();
        assert!(response.success);
    }
}
```

---

## 端到端测试

### HTTP API 测试

```rust
#[cfg(test)]
mod e2e_tests {
    use actix_web::{test, App};
    use qaexchange::service::http::routes;

    #[actix_web::test]
    async fn test_open_account_api() {
        // 创建测试应用
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(app_state))
                .configure(routes::config)
        ).await;

        // 发送开户请求
        let req = test::TestRequest::post()
            .uri("/api/account/open")
            .set_json(&json!({
                "user_id": "test001",
                "user_name": "Test User",
                "init_cash": 1000000.0,
                "account_type": "individual",
                "password": "password123"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // 验证响应
        let body: ApiResponse = test::read_body_json(resp).await;
        assert!(body.success);
    }
}
```

### WebSocket 测试

```rust
#[actix_web::test]
async fn test_websocket_connection() {
    use actix_web_actors::ws;

    let mut srv = test::start(|| {
        App::new()
            .route("/ws", web::get().to(ws_route))
    });

    // 连接 WebSocket
    let mut framed = srv.ws_at("/ws?user_id=test_user").await.unwrap();

    // 发送认证消息
    framed.send(ws::Message::Text(
        json!({
            "type": "auth",
            "user_id": "test_user",
            "token": "test_token"
        }).to_string().into()
    )).await.unwrap();

    // 接收响应
    let response = framed.next().await.unwrap().unwrap();
    // 验证响应...
}
```

---

## 测试覆盖率

### 使用 Tarpaulin

**安装**:
```bash
cargo install cargo-tarpaulin
```

**运行覆盖率测试**:
```bash
# 生成覆盖率报告
cargo tarpaulin --lib --out Html

# 查看报告
open tarpaulin-report.html

# 指定最小覆盖率
cargo tarpaulin --lib --fail-under 80
```

### 使用 llvm-cov

```bash
# 安装
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov

# 运行
cargo llvm-cov --html

# 查看报告
open target/llvm-cov/html/index.html
```

### CI 集成

**GitHub Actions**:
```yaml
# .github/workflows/test.yml
name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v3

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run tests
        run: cargo test --lib

      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin

      - name: Generate coverage
        run: cargo tarpaulin --lib --out Xml

      - name: Upload coverage
        uses: codecov/codecov-action@v3
        with:
          files: ./cobertura.xml
```

---

## 最佳实践

### 1. 测试命名

```rust
// ✅ 好的命名
#[test]
fn test_submit_order_with_insufficient_funds_should_fail() { }

#[test]
fn test_cancel_order_returns_ok_for_valid_order() { }

// ❌ 不好的命名
#[test]
fn test1() { }

#[test]
fn test_order() { }
```

### 2. 测试隔离

```rust
// ✅ 每个测试独立创建数据
#[test]
fn test_a() {
    let data = create_test_data();
    // ...
}

#[test]
fn test_b() {
    let data = create_test_data();  // 独立数据
    // ...
}

// ❌ 测试共享状态
static mut SHARED_DATA: Option<Data> = None;  // 不推荐
```

### 3. 使用测试 Fixture

```rust
// 提取公共设置逻辑
fn create_test_router() -> OrderRouter {
    // 公共初始化逻辑
}

#[test]
fn test_case_1() {
    let router = create_test_router();
    // 测试逻辑
}

#[test]
fn test_case_2() {
    let router = create_test_router();
    // 测试逻辑
}
```

### 4. 测试边界条件

```rust
#[test]
fn test_order_volume_boundaries() {
    // 最小值
    test_volume(0.0);  // 应失败
    test_volume(1.0);  // 应成功

    // 最大值
    test_volume(9999.0);   // 应成功
    test_volume(10000.0);  // 应成功
    test_volume(10001.0);  // 应失败
}
```

### 5. 使用 Mock

```rust
// 使用 mockall crate
use mockall::{automock, predicate::*};

#[automock]
trait AccountManager {
    fn get_account(&self, user_id: &str) -> Result<Account, Error>;
}

#[test]
fn test_with_mock() {
    let mut mock = MockAccountManager::new();
    mock.expect_get_account()
        .with(eq("user001"))
        .times(1)
        .returning(|_| Ok(create_test_account()));

    // 使用 mock
    let result = function_under_test(&mock);
    assert!(result.is_ok());
}
```

### 6. 快速失败

```rust
// ✅ 尽早返回
#[test]
fn test_complex_workflow() {
    let result1 = step1();
    assert!(result1.is_ok(), "Step 1 failed");

    let result2 = step2();
    assert!(result2.is_ok(), "Step 2 failed");

    // ...
}

// ❌ 全部执行后才断言
#[test]
fn test_complex_workflow_bad() {
    let result1 = step1();
    let result2 = step2();
    let result3 = step3();

    assert!(result1.is_ok() && result2.is_ok() && result3.is_ok());
}
```

---

## 测试工具

### 常用 Crate

| Crate | 用途 |
|-------|------|
| `criterion` | 基准测试 |
| `mockall` | Mock 对象 |
| `proptest` | 属性测试 |
| `rstest` | 参数化测试 |
| `serial_test` | 串行测试 |
| `test-case` | 测试用例生成 |

### 参数化测试

```rust
use rstest::rstest;

#[rstest]
#[case(10.0, 120.0, true)]   // 正常订单
#[case(0.0, 120.0, false)]   // 数量为0
#[case(10.0, -1.0, false)]   // 负价格
fn test_order_validation(
    #[case] volume: f64,
    #[case] price: f64,
    #[case] expected: bool
) {
    let result = validate_order(volume, price);
    assert_eq!(result.is_ok(), expected);
}
```

---

## 持续集成

### 本地 CI 模拟

```bash
#!/bin/bash
# ci-check.sh

set -e

echo "Running fmt check..."
cargo fmt --check

echo "Running clippy..."
cargo clippy -- -D warnings

echo "Running tests..."
cargo test --lib

echo "Checking coverage..."
cargo tarpaulin --lib --fail-under 70

echo "All checks passed!"
```

### Pre-commit Hook

```bash
# .git/hooks/pre-commit
#!/bin/bash

cargo fmt --check || {
    echo "Code not formatted. Run 'cargo fmt' first."
    exit 1
}

cargo clippy -- -D warnings || {
    echo "Clippy errors found."
    exit 1
}

cargo test --lib || {
    echo "Tests failed."
    exit 1
}
```

---

**文档更新**: 2025-10-03
**维护者**: @yutiansut
