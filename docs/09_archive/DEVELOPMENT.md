# 开发指南

**版本**: v0.1.0
**更新日期**: 2025-10-03
**开发团队**: @yutiansut

---

## 📋 目录

1. [开发环境搭建](#开发环境搭建)
2. [项目结构](#项目结构)
3. [开发工作流](#开发工作流)
4. [代码规范](#代码规范)
5. [常用命令](#常用命令)
6. [调试技巧](#调试技巧)
7. [贡献指南](#贡献指南)

---

## 开发环境搭建

### 1. 安装 Rust

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 选择默认安装
source $HOME/.cargo/env

# 验证安装
rustc --version  # rustc 1.75.0
cargo --version  # cargo 1.75.0
```

### 2. 安装工具链

```bash
# 格式化工具
rustup component add rustfmt

# Clippy 代码检查
rustup component add clippy

# Rust Analyzer (LSP)
# VSCode: 安装 rust-analyzer 插件
# 其他编辑器: https://rust-analyzer.github.io/
```

### 3. 克隆项目

```bash
git clone https://github.com/quantaxis/qaexchange-rs.git
cd qaexchange-rs
```

### 4. 克隆依赖项目 (qars)

```bash
# 在同级目录克隆 qars
cd ..
git clone https://github.com/quantaxis/qars2.git
cd qaexchange-rs

# 验证 Cargo.toml 中的路径
# [dependencies]
# qars = { path = "../qars2", package = "qa-rs" }
```

### 5. 构建项目

```bash
# 检查编译
cargo check --lib

# 开发构建
cargo build

# 运行测试
cargo test --lib
```

### 6. IDE 配置

**VSCode 推荐插件**:
- **rust-analyzer**: Rust 语言支持
- **CodeLLDB**: 调试支持
- **Even Better TOML**: TOML 语法高亮
- **Error Lens**: 内联错误显示

**VSCode 配置** (`.vscode/settings.json`):
```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.cargo.features": "all",
  "editor.formatOnSave": true,
  "editor.rulers": [100],
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

**调试配置** (`.vscode/launch.json`):
```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug qaexchange-rs",
      "cargo": {
        "args": ["build", "--bin", "qaexchange-rs"],
        "filter": {
          "name": "qaexchange-rs",
          "kind": "bin"
        }
      },
      "args": [],
      "cwd": "${workspaceFolder}",
      "env": {
        "RUST_LOG": "debug"
      }
    }
  ]
}
```

---

## 项目结构

```
qaexchange-rs/
├── Cargo.toml              # 项目配置
├── Cargo.lock              # 依赖锁定
├── src/
│   ├── lib.rs              # 库入口，定义 ExchangeError
│   ├── core/               # 核心层
│   │   ├── mod.rs          # 导出 AccountManager
│   │   └── account_ext.rs  # 账户扩展功能
│   ├── exchange/           # 业务层
│   │   ├── mod.rs
│   │   ├── order_router.rs       # 订单路由器
│   │   ├── trade_gateway.rs      # 成交网关
│   │   └── settlement.rs         # 结算引擎
│   ├── risk/               # 风控层
│   │   ├── mod.rs
│   │   └── pre_trade_check.rs    # 盘前风控
│   ├── service/            # 服务层
│   │   ├── mod.rs
│   │   ├── http/                 # HTTP API
│   │   │   ├── mod.rs
│   │   │   ├── models.rs         # 数据模型
│   │   │   ├── handlers.rs       # 请求处理器
│   │   │   └── routes.rs         # 路由配置
│   │   └── websocket/            # WebSocket
│   │       ├── mod.rs
│   │       ├── messages.rs       # 消息协议
│   │       ├── session.rs        # 会话管理
│   │       └── handler.rs        # 业务处理器
│   └── bin/
│       └── qaexchange-rs.rs      # 主程序 (未实现)
├── docs/                   # 文档
│   ├── README.md
│   ├── API_REFERENCE.md
│   ├── WEBSOCKET_PROTOCOL.md
│   ├── ARCHITECTURE.md
│   ├── DEPLOYMENT.md
│   ├── DEVELOPMENT.md          # 本文档
│   ├── TESTING.md
│   ├── PERFORMANCE.md
│   ├── ERROR_CODES.md
│   └── FRONTEND_INTEGRATION.md
└── tests/                  # 集成测试 (未实现)
```

---

## 开发工作流

### 1. 创建功能分支

```bash
# 从 master 创建分支
git checkout -b feature/your-feature-name

# 或修复 bug
git checkout -b fix/bug-description
```

### 2. 开发功能

**编写代码**:
```rust
// src/exchange/my_feature.rs

use crate::ExchangeError;

pub struct MyFeature {
    // ...
}

impl MyFeature {
    pub fn new() -> Self {
        Self { /* ... */ }
    }

    pub fn do_something(&self) -> Result<(), ExchangeError> {
        // 实现逻辑
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        let feature = MyFeature::new();
        assert!(feature.do_something().is_ok());
    }
}
```

**导出模块**:
```rust
// src/exchange/mod.rs
pub mod my_feature;
pub use my_feature::MyFeature;
```

### 3. 运行检查

```bash
# 格式化代码
cargo fmt

# Clippy 检查
cargo clippy -- -D warnings

# 编译检查
cargo check --lib

# 运行测试
cargo test --lib

# 查看测试覆盖率 (需要 tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin --lib
```

### 4. 提交代码

```bash
# 暂存更改
git add .

# 提交 (遵循 Conventional Commits)
git commit -m "feat: add new feature description"
git commit -m "fix: fix bug description"
git commit -m "docs: update documentation"

# 推送分支
git push origin feature/your-feature-name
```

### 5. 创建 Pull Request

- 在 GitHub 上创建 PR
- 填写 PR 描述
- 等待 CI 检查通过
- 代码审查
- 合并到 master

---

## 代码规范

### Rust 代码风格

**遵循官方风格指南**:
- 使用 `rustfmt` 自动格式化
- 缩进: 4 个空格
- 行宽: 100 字符
- 导入顺序: std → 第三方 → 本地

**命名规范**:
```rust
// 结构体: PascalCase
pub struct OrderRouter { }

// 函数/方法: snake_case
pub fn submit_order() { }

// 常量: SCREAMING_SNAKE_CASE
const MAX_ORDERS: usize = 10000;

// 枚举: PascalCase
pub enum OrderStatus {
    Pending,    // 变体: PascalCase
    Accepted,
}
```

**错误处理**:
```rust
// ✅ 使用 Result
pub fn get_account(&self, user_id: &str) -> Result<Arc<RwLock<QA_Account>>, ExchangeError> {
    self.accounts.get(user_id)
        .map(|acc| acc.clone())
        .ok_or_else(|| ExchangeError::AccountNotFound(user_id.to_string()))
}

// ❌ 避免 unwrap/expect (除非在测试中)
let account = self.accounts.get(user_id).unwrap();  // 不推荐

// ✅ 使用 ? 传播错误
pub fn process(&self) -> Result<(), ExchangeError> {
    let account = self.get_account(user_id)?;
    Ok(())
}
```

**日志规范**:
```rust
use log::{trace, debug, info, warn, error};

// trace: 详细跟踪信息
log::trace!("订单簿快照: {:?}", orderbook);

// debug: 调试信息
log::debug!("订单 {} 提交成功", order_id);

// info: 重要信息
log::info!("账户 {} 开户成功，初始资金 {}", user_id, init_cash);

// warn: 警告
log::warn!("风险度 {:.2}% 接近阈值", risk_ratio * 100.0);

// error: 错误
log::error!("撮合引擎异常: {}", error);
```

### 文档注释

```rust
/// 订单路由器
///
/// 负责接收订单、风控检查、路由到撮合引擎，并处理撮合结果。
///
/// # Examples
///
/// ```
/// use qaexchange::exchange::OrderRouter;
///
/// let router = OrderRouter::new(account_mgr, risk_checker, trade_gateway);
/// let response = router.submit_order(req);
/// ```
pub struct OrderRouter {
    // ...
}

impl OrderRouter {
    /// 提交订单
    ///
    /// # Arguments
    ///
    /// * `req` - 订单提交请求
    ///
    /// # Returns
    ///
    /// 返回订单提交响应，包含订单ID或错误信息
    pub fn submit_order(&self, req: SubmitOrderRequest) -> SubmitOrderResponse {
        // ...
    }
}
```

---

## 常用命令

### 构建命令

```bash
# 快速检查编译
cargo check

# 检查库 (不包括 bin)
cargo check --lib

# 开发构建
cargo build

# 生产构建
cargo build --release

# 增量构建 (更快)
cargo build --incremental
```

### 测试命令

```bash
# 运行所有测试
cargo test

# 运行库测试 (不包括集成测试)
cargo test --lib

# 运行特定测试
cargo test order_router

# 显示测试输出
cargo test -- --nocapture

# 并行测试
cargo test -- --test-threads=4

# 生成测试报告
cargo test -- --format=json > test_results.json
```

### 代码质量

```bash
# 格式化代码
cargo fmt

# 检查格式 (CI 中使用)
cargo fmt --check

# Clippy 检查
cargo clippy

# Clippy 严格模式
cargo clippy -- -D warnings

# 查看未使用依赖
cargo install cargo-udeps
cargo +nightly udeps
```

### 依赖管理

```bash
# 查看依赖树
cargo tree

# 更新依赖
cargo update

# 添加依赖
cargo add serde

# 移除依赖
cargo remove serde
```

### 性能分析

```bash
# 编译优化
RUSTFLAGS="-C target-cpu=native" cargo build --release

# 查看编译时间
cargo build --release --timings

# 基准测试 (需要 criterion)
cargo bench

# 火焰图 (需要 flamegraph)
cargo install flamegraph
cargo flamegraph --bin qaexchange-rs
```

---

## 调试技巧

### 1. 日志调试

```bash
# 设置日志级别
export RUST_LOG=debug
cargo run

# 模块级日志
export RUST_LOG=qaexchange::exchange::order_router=trace
cargo run

# 多模块日志
export RUST_LOG=qaexchange::exchange=debug,actix_web=info
cargo run
```

### 2. GDB/LLDB 调试

```bash
# 生成 debug 符号
cargo build

# 使用 rust-gdb
rust-gdb target/debug/qaexchange-rs

# 或 rust-lldb
rust-lldb target/debug/qaexchange-rs

# 常用命令
(gdb) break src/exchange/order_router.rs:100  # 设置断点
(gdb) run                                      # 运行
(gdb) next                                     # 下一行
(gdb) step                                     # 进入函数
(gdb) print order_id                           # 打印变量
(gdb) backtrace                                # 调用栈
```

### 3. 单元测试调试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug() {
        // 使用 dbg! 宏打印调试信息
        let order_id = "O12345";
        dbg!(order_id);

        // assert_eq! 会显示详细差异
        assert_eq!(actual, expected);

        // 打印到 stderr (cargo test -- --nocapture)
        eprintln!("Debug: {:?}", some_value);
    }
}
```

### 4. 性能调试

```bash
# 使用 perf (Linux)
cargo build --release
perf record -g ./target/release/qaexchange-rs
perf report

# 使用 valgrind
cargo build
valgrind --tool=callgrind ./target/debug/qaexchange-rs
kcachegrind callgrind.out.*
```

---

## 贡献指南

### 提交规范

**Conventional Commits**:
```
<type>(<scope>): <subject>

<body>

<footer>
```

**类型 (type)**:
- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `style`: 代码格式 (不影响功能)
- `refactor`: 重构
- `perf`: 性能优化
- `test`: 测试相关
- `chore`: 构建/工具链

**示例**:
```bash
git commit -m "feat(order_router): add order cancellation batch API"

git commit -m "fix(websocket): fix heartbeat timeout issue

The WebSocket heartbeat was not properly resetting the timeout,
causing premature disconnections.

Fixes #123"

git commit -m "docs(api): update REST API examples"
```

### Code Review 要点

**提交 PR 前检查**:
- [ ] 代码格式化 (`cargo fmt`)
- [ ] Clippy 检查通过 (`cargo clippy`)
- [ ] 所有测试通过 (`cargo test --lib`)
- [ ] 添加了必要的测试
- [ ] 更新了相关文档
- [ ] Commit 消息规范

**Review 关注点**:
- 代码逻辑正确性
- 错误处理完整性
- 性能影响
- 代码可读性
- 测试覆盖率

### 发布流程

```bash
# 1. 更新版本号
# 修改 Cargo.toml
version = "0.2.0"

# 2. 更新 CHANGELOG.md
# 记录变更内容

# 3. 提交版本更新
git add Cargo.toml CHANGELOG.md
git commit -m "chore: bump version to 0.2.0"

# 4. 打标签
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0

# 5. 发布 (如果发布到 crates.io)
cargo publish
```

---

## 常见问题

### Q1: 编译很慢怎么办?

**A**: 使用增量编译和并行构建
```bash
# .cargo/config.toml
[build]
incremental = true
jobs = 8  # 根据 CPU 核心数调整

# 使用 sccache 缓存
cargo install sccache
export RUSTC_WRAPPER=sccache
```

### Q2: 如何提高开发效率?

**A**: 使用 `cargo-watch` 自动重新编译
```bash
cargo install cargo-watch

# 自动运行测试
cargo watch -x test

# 自动运行 check
cargo watch -x check
```

### Q3: 如何调试宏展开?

**A**: 使用 `cargo expand`
```bash
cargo install cargo-expand

# 展开指定模块的宏
cargo expand exchange::order_router
```

### Q4: 如何查看依赖更新?

**A**: 使用 `cargo-outdated`
```bash
cargo install cargo-outdated

# 查看过时依赖
cargo outdated
```

---

## 推荐资源

### 官方文档
- [The Rust Programming Language](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

### 第三方库文档
- [Actix-web](https://actix.rs/)
- [Tokio](https://tokio.rs/)
- [Serde](https://serde.rs/)

### 社区
- [Rust 官方论坛](https://users.rust-lang.org/)
- [Rust Reddit](https://www.reddit.com/r/rust/)
- [Rust 中文社区](https://rustcc.cn/)

---

**文档更新**: 2025-10-03
**维护者**: @yutiansut
