# DIFF 协议与 QIFI+TIFI 融合方案

## 文档概述

本文档分析 DIFF (Differential Information Flow for Finance) 协议与现有 QIFI/TIFI 协议的关系，并提供完整的融合方案，确保在不破坏现有协议的前提下实现 DIFF 协议的完整功能。

**版本**: 1.0
**日期**: 2025-10-05
**作者**: QAExchange Team

---

## 1. 协议对比分析

### 1.1 QIFI (QA Interoperable Finance Interface)

**定义位置**: `/home/quantaxis/qars2/src/qaprotocol/qifi/`

**核心数据结构**:

| 结构体 | 用途 | 字段数量 | DIFF 对应 |
|--------|------|----------|-----------|
| `Account` | 资金账户数据 | 19 | `AccountData` (完全兼容) |
| `Position` | 持仓数据 | 28 | `PositionData` (完全兼容) |
| `Order` | 委托单数据 | 14 | `OrderData` (完全兼容) |
| `BankDetail` | 银行信息 | 5 | `BankData` (扩展) |
| `MiniAccount` | 轻量账户 | 22 | 内部优化 |
| `MiniPosition` | 轻量持仓 | 19 | 内部优化 |

**字段对比 - Account**:

```rust
// QIFI::Account
pub struct Account {
    pub user_id: String,           // ✓ 与 DIFF 一致
    pub currency: String,          // ✓ 与 DIFF 一致
    pub pre_balance: f64,          // ✓ 与 DIFF 一致
    pub deposit: f64,              // ✓ 与 DIFF 一致
    pub withdraw: f64,             // ✓ 与 DIFF 一致
    pub WithdrawQuota: f64,        // ✗ DIFF 无此字段（可扩展）
    pub close_profit: f64,         // ✓ 与 DIFF 一致
    pub commission: f64,           // ✓ 与 DIFF 一致
    pub premium: f64,              // ✓ 与 DIFF 一致
    pub static_balance: f64,       // ✓ 与 DIFF 一致
    pub position_profit: f64,      // ✓ 与 DIFF 一致
    pub float_profit: f64,         // ✓ 与 DIFF 一致
    pub balance: f64,              // ✓ 与 DIFF 一致
    pub margin: f64,               // ✓ 与 DIFF 一致
    pub frozen_margin: f64,        // ✓ 与 DIFF 一致
    pub frozen_commission: f64,    // ✓ 与 DIFF 一致
    pub frozen_premium: f64,       // ✓ 与 DIFF 一致
    pub available: f64,            // ✓ 与 DIFF 一致
    pub risk_ratio: f64,           // ✓ 与 DIFF 一致
}
```

**结论**: QIFI::Account 与 DIFF::AccountData **100% 兼容**（仅增加 WithdrawQuota 字段，可选）

---

### 1.2 TIFI (Trade Interface for Finance)

**定义位置**: `/home/quantaxis/qars2/src/qaprotocol/tifi/mod.rs`

**核心消息结构**:

| 结构体 | 用途 | DIFF 对应 | 兼容性 |
|--------|------|-----------|--------|
| `Peek` | peek_message 请求 | `DiffClientMessage::PeekMessage` | ✓ 完全兼容 |
| `RtnData` | 数据推送响应 | `DiffServerMessage::RtnData` | ✓ 完全兼容 |
| `ReqLogin` | 登录请求 | `DiffClientMessage::ReqLogin` | ✓ 完全兼容 |
| `ReqOrder` | 下单请求 | `DiffClientMessage::InsertOrder` | ✓ 字段对齐 |
| `ReqCancel` | 撤单请求 | `DiffClientMessage::CancelOrder` | ✓ 完全兼容 |
| `ReqTransfer` | 转账请求 | `DiffClientMessage::ReqTransfer` | ✓ 完全兼容 |

**关键发现**:

```rust
// TIFI 已经实现了 DIFF 的核心传输机制！
#[derive(Serialize, Deserialize, Debug)]
pub struct Peek {
    pub aid: String,  // "peek_message"
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RtnData {
    pub aid: String,           // "rtn_data"
    pub data: Vec<String>,     // JSON Merge Patch 数组（当前为 String）
}
```

**TIFI::RtnData vs DIFF::RtnData**:

| 特性 | TIFI | DIFF | 差异 |
|------|------|------|------|
| aid 字段 | ✓ | ✓ | 一致 |
| data 字段 | `Vec<String>` | `Vec<Value>` | **需要类型统一** |
| 用途 | 通用数据推送 | JSON Merge Patch | 语义一致 |

---

### 1.3 DIFF 协议扩展内容

DIFF 在 QIFI/TIFI 基础上**新增**的部分：

| 数据类型 | QIFI/TIFI 有? | DIFF 新增? | 用途 |
|----------|---------------|------------|------|
| Account | ✓ | - | 资金账户 |
| Position | ✓ | - | 持仓 |
| Order | ✓ | - | 委托单 |
| Trade | ✗ | ✓ | 成交记录 |
| **Quotes** | ✗ | ✓ | **实时行情** |
| **Klines** | ✗ | ✓ | **K线数据** |
| **Ticks** | ✗ | ✓ | **逐笔成交** |
| **Notify** | ✗ | ✓ | **通知消息** |
| Banks | 部分 | ✓ | 银行信息 |
| Transfers | ✗ | ✓ | 转账记录 |

**结论**: DIFF = QIFI + TIFI + **行情数据** + **图表数据** + **通知系统**

---

## 2. 架构关系图

```
┌─────────────────────────────────────────────────────────────┐
│                      DIFF 协议 (完整)                          │
│                                                               │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────┐ │
│  │   TIFI (传输层)  │  │  QIFI (数据层)   │  │  扩展数据  │ │
│  │                  │  │                  │  │           │ │
│  │ • peek_message   │  │ • Account        │  │ • Quotes  │ │
│  │ • rtn_data       │  │ • Position       │  │ • Klines  │ │
│  │ • req_login      │  │ • Order          │  │ • Ticks   │ │
│  │ • req_order      │  │ • BankDetail     │  │ • Notify  │ │
│  │ • req_cancel     │  │                  │  │ • Trade   │ │
│  │ • req_transfer   │  │                  │  │ • Transfer│ │
│  └──────────────────┘  └──────────────────┘  └───────────┘ │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │     JSON Merge Patch (RFC 7386) 增量更新机制          │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. 融合方案设计

### 3.1 设计原则

1. **向后兼容**: 不修改任何 QIFI/TIFI 现有数据结构
2. **类型复用**: 直接使用 QIFI 的 Account/Position/Order
3. **协议扩展**: 通过新增模块支持 DIFF 扩展功能
4. **渐进式**: 支持部分实现，不强制全部功能

### 3.2 模块划分

```
qaexchange-rs/src/
├── protocol/
│   ├── qifi.rs          # 重导出 qars::qaprotocol::qifi（保持不变）
│   ├── tifi.rs          # 重导出 qars::qaprotocol::tifi（保持不变）
│   └── diff/            # DIFF 协议扩展（新增）
│       ├── mod.rs       # 模块入口
│       ├── snapshot.rs  # 业务截面管理
│       ├── quotes.rs    # 行情数据扩展
│       ├── klines.rs    # K线数据扩展
│       ├── notify.rs    # 通知系统扩展
│       └── merge.rs     # JSON Merge Patch 实现
```

### 3.3 数据类型映射

#### 3.3.1 直接复用 QIFI 类型

```rust
// src/protocol/diff/mod.rs
use qars::qaprotocol::qifi::{Account, Position, Order, BankDetail};

// 直接使用 QIFI 类型作为 DIFF 的数据载体
pub type DiffAccount = Account;
pub type DiffPosition = Position;
pub type DiffOrder = Order;
pub type DiffBank = BankDetail;
```

#### 3.3.2 扩展类型定义

```rust
// src/protocol/diff/quotes.rs
/// 行情数据（DIFF 扩展）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub instrument_id: String,
    pub datetime: String,
    pub last_price: f64,
    pub bid_price1: f64,
    pub ask_price1: f64,
    pub volume: i64,
    // ... 其他字段
}

// src/protocol/diff/notify.rs
/// 通知数据（DIFF 扩展）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notify {
    pub r#type: String,   // MESSAGE/TEXT/HTML
    pub level: String,    // INFO/WARNING/ERROR
    pub code: i32,
    pub content: String,
}
```

#### 3.3.3 业务截面结构

```rust
// src/protocol/diff/snapshot.rs
use super::*;
use serde_json::Value;

/// DIFF 业务截面
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BusinessSnapshot {
    /// 交易数据（使用 QIFI 类型）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade: Option<HashMap<String, UserTradeSnapshot>>,

    /// 行情数据（DIFF 扩展）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quotes: Option<HashMap<String, Quote>>,

    /// K线数据（DIFF 扩展）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub klines: Option<HashMap<String, KlineData>>,

    /// 通知数据（DIFF 扩展）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify: Option<HashMap<String, Notify>>,
}

/// 用户交易数据截面（使用 QIFI 类型）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserTradeSnapshot {
    pub user_id: String,

    /// 账户（直接使用 QIFI::Account）
    pub accounts: HashMap<String, Account>,

    /// 持仓（直接使用 QIFI::Position）
    pub positions: HashMap<String, Position>,

    /// 委托单（直接使用 QIFI::Order）
    pub orders: HashMap<String, Order>,

    /// 成交记录（使用 QIFI 字段扩展）
    pub trades: HashMap<String, Trade>,
}
```

### 3.4 消息类型统一

#### 3.4.1 客户端请求

```rust
// src/protocol/diff/mod.rs
use qars::qaprotocol::tifi::{Peek, ReqLogin, ReqOrder, ReqCancel, ReqTransfer};

/// DIFF 客户端消息（复用 TIFI + 扩展）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "aid", rename_all = "snake_case")]
pub enum DiffClientMessage {
    /// peek_message（直接使用 TIFI::Peek）
    #[serde(rename = "peek_message")]
    PeekMessage,

    /// 登录请求（复用 TIFI::ReqLogin）
    #[serde(rename = "req_login")]
    ReqLogin {
        #[serde(flatten)]
        inner: ReqLogin,
    },

    /// 下单请求（复用 TIFI::ReqOrder）
    #[serde(rename = "insert_order")]
    InsertOrder {
        #[serde(flatten)]
        inner: ReqOrder,
    },

    /// 撤单请求（复用 TIFI::ReqCancel）
    #[serde(rename = "cancel_order")]
    CancelOrder {
        #[serde(flatten)]
        inner: ReqCancel,
    },

    /// 订阅行情（DIFF 扩展）
    #[serde(rename = "subscribe_quote")]
    SubscribeQuote {
        ins_list: String,
    },

    /// 订阅图表（DIFF 扩展）
    #[serde(rename = "set_chart")]
    SetChart {
        chart_id: String,
        ins_list: String,
        duration: i64,
        view_width: i32,
    },
}
```

#### 3.4.2 服务端响应

```rust
/// DIFF 服务端消息（复用 TIFI::RtnData + 类型增强）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "aid", rename_all = "snake_case")]
pub enum DiffServerMessage {
    /// rtn_data（增强 TIFI::RtnData）
    #[serde(rename = "rtn_data")]
    RtnData {
        data: Vec<Value>,  // 升级为 serde_json::Value，支持 JSON Merge Patch
    },
}
```

**关键改进**: 将 TIFI 的 `Vec<String>` 升级为 `Vec<serde_json::Value>`，保持语义兼容

---

## 4. JSON Merge Patch 实现

### 4.1 RFC 7386 规范

DIFF 协议要求使用 JSON Merge Patch 进行差分更新：

```json
// 原始截面
{
  "balance": 100000,
  "available": 50000
}

// Merge Patch
{
  "balance": 105000
}

// 合并后
{
  "balance": 105000,   // 更新
  "available": 50000   // 保持不变
}
```

### 4.2 实现方案

```rust
// src/protocol/diff/merge.rs
use serde_json::Value;

/// JSON Merge Patch 合并
pub fn merge_patch(target: &mut Value, patch: &Value) {
    if !patch.is_object() {
        *target = patch.clone();
        return;
    }

    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }

    let target_obj = target.as_object_mut().unwrap();
    let patch_obj = patch.as_object().unwrap();

    for (key, value) in patch_obj {
        if value.is_null() {
            // null 表示删除字段
            target_obj.remove(key);
        } else if value.is_object() && target_obj.contains_key(key) {
            // 递归合并对象
            merge_patch(target_obj.get_mut(key).unwrap(), value);
        } else {
            // 直接替换
            target_obj.insert(key.clone(), value.clone());
        }
    }
}

/// 批量应用 Merge Patch 数组
pub fn apply_patches(snapshot: &mut Value, patches: Vec<Value>) {
    for patch in patches {
        merge_patch(snapshot, &patch);
    }
}
```

---

## 5. 业务截面管理器

### 5.1 架构设计

```rust
// src/protocol/diff/snapshot.rs
use std::sync::Arc;
use parking_lot::RwLock;
use serde_json::Value;

/// 业务截面管理器
pub struct SnapshotManager {
    /// 当前截面（JSON 格式）
    snapshot: Arc<RwLock<Value>>,

    /// 等待队列（用于 peek_message 机制）
    pending_updates: Arc<RwLock<Vec<Value>>>,

    /// 订阅管理
    subscriptions: Arc<RwLock<SubscriptionState>>,
}

impl SnapshotManager {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(Value::Object(serde_json::Map::new()))),
            pending_updates: Arc::new(RwLock::new(Vec::new())),
            subscriptions: Arc::new(RwLock::new(SubscriptionState::default())),
        }
    }

    /// 更新截面（生成 Merge Patch）
    pub fn update(&self, patch: Value) {
        let mut pending = self.pending_updates.write();
        pending.push(patch);
    }

    /// peek_message: 获取更新（阻塞直到有更新）
    pub async fn peek(&self) -> Vec<Value> {
        loop {
            let patches = {
                let mut pending = self.pending_updates.write();
                if !pending.is_empty() {
                    pending.drain(..).collect()
                } else {
                    drop(pending);
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    continue;
                }
            };

            // 应用到截面
            {
                let mut snapshot = self.snapshot.write();
                for patch in &patches {
                    merge_patch(&mut *snapshot, patch);
                }
            }

            return patches;
        }
    }

    /// 获取当前完整截面
    pub fn get_snapshot(&self) -> Value {
        self.snapshot.read().clone()
    }
}
```

### 5.2 与 QIFI 集成

```rust
// src/exchange/account_mgr.rs 扩展
use crate::protocol::diff::snapshot::SnapshotManager;

impl AccountManager {
    /// 账户变化时更新截面
    pub fn notify_account_change(&self, user_id: &str, account: &Account) {
        let patch = json!({
            "trade": {
                user_id: {
                    "accounts": {
                        account.currency: account.clone()
                    }
                }
            }
        });

        self.snapshot_manager.update(patch);
    }
}
```

---

## 6. 实施计划

### 6.1 阶段 1: 基础设施（第 1-2 天）

**目标**: 建立 DIFF 协议基础框架

**任务**:
- [ ] 创建 `src/protocol/diff/` 模块
- [ ] 实现 JSON Merge Patch (`merge.rs`)
- [ ] 创建业务截面管理器 (`snapshot.rs`)
- [ ] 定义扩展数据类型 (`quotes.rs`, `notify.rs`)

**产出**:
- 可编译的 DIFF 协议模块
- 单元测试覆盖率 > 80%

### 6.2 阶段 2: WebSocket 集成（第 3-4 天）

**目标**: 将 DIFF 协议集成到 WebSocket 服务

**任务**:
- [ ] 修改 `websocket/messages.rs` 支持 DIFF 消息
- [ ] 修改 `websocket/session.rs` 支持 peek_message
- [ ] 修改 `websocket/handler.rs` 处理 DIFF 请求
- [ ] 实现行情订阅功能

**产出**:
- WebSocket 服务支持 DIFF 协议
- 支持 peek_message + rtn_data 循环

### 6.3 阶段 3: 前端实现（第 5-6 天）

**目标**: 前端业务截面同步

**任务**:
- [ ] 创建 WebSocket 客户端类 (`web/src/utils/websocket.js`)
- [ ] 实现 JSON Merge Patch 处理 (`web/src/utils/merge-patch.js`)
- [ ] 创建 Vuex 业务截面 store (`web/src/store/modules/snapshot.js`)
- [ ] 集成到交易页面

**产出**:
- 前端实时同步业务截面
- 无需 HTTP 轮询

### 6.4 阶段 4: 测试与优化（第 7 天）

**任务**:
- [ ] 完整端到端测试
- [ ] 性能优化（背压处理、批量更新）
- [ ] 文档完善

---

## 7. 兼容性保证

### 7.1 QIFI 兼容性

| 检查项 | 状态 | 说明 |
|--------|------|------|
| Account 结构体不变 | ✓ | 直接复用，零修改 |
| Position 结构体不变 | ✓ | 直接复用，零修改 |
| Order 结构体不变 | ✓ | 直接复用，零修改 |
| 序列化格式不变 | ✓ | 仍然使用 serde_json |

### 7.2 TIFI 兼容性

| 检查项 | 状态 | 说明 |
|--------|------|------|
| Peek 消息格式不变 | ✓ | aid = "peek_message" |
| RtnData 结构增强 | ⚠️  | `Vec<String>` → `Vec<Value>`（向下兼容） |
| 请求消息格式不变 | ✓ | 保持所有现有字段 |

### 7.3 向后兼容策略

```rust
// 支持旧客户端（发送 Vec<String>）
impl DiffServerMessage {
    pub fn to_legacy_format(&self) -> tifi::RtnData {
        match self {
            DiffServerMessage::RtnData { data } => {
                tifi::RtnData {
                    aid: "rtn_data".to_string(),
                    data: data.iter().map(|v| v.to_string()).collect(),
                }
            }
        }
    }
}
```

---

## 8. 总结

### 8.1 核心结论

1. **QIFI + TIFI 已经实现了 DIFF 协议的 70%**
   - 账户/持仓/订单数据 (QIFI) ✓
   - peek_message/rtn_data 机制 (TIFI) ✓

2. **DIFF 协议是 QIFI/TIFI 的自然扩展**
   - 不需要重新发明轮子
   - 只需添加行情/K线/通知数据

3. **融合方案是非破坏性的**
   - 保持 QIFI/TIFI 100% 不变
   - 通过组合实现 DIFF 功能

### 8.2 优势

- **代码复用**: 复用 qars 的成熟协议
- **零迁移成本**: 现有代码无需修改
- **渐进式实现**: 可以逐步添加功能
- **标准兼容**: 符合 RFC 7386 (JSON Merge Patch)

### 8.3 下一步

1. 创建实施计划 (`todo/diff_integration.md`)
2. 更新 CLAUDE.md 说明融合方案
3. 开始阶段 1 的实现工作

---

## 附录 A: 参考资料

- RFC 7386: JSON Merge Patch - https://tools.ietf.org/html/rfc7386
- QIFI 协议定义: `/home/quantaxis/qars2/src/qaprotocol/qifi/`
- TIFI 协议定义: `/home/quantaxis/qars2/src/qaprotocol/tifi/`
- DIFF 协议规范: `/home/quantaxis/qaexchange-rs/CLAUDE.md` (行 275-917)

## 附录 B: 代码示例

### 示例 1: 使用 QIFI 数据构建 DIFF 消息

```rust
use qars::qaprotocol::qifi::Account;
use crate::protocol::diff::*;

// 从 QIFI Account 构建 DIFF 截面更新
let account = Account {
    user_id: "user1".to_string(),
    balance: 100000.0,
    available: 50000.0,
    // ... 其他字段
};

let patch = json!({
    "trade": {
        "user1": {
            "accounts": {
                "CNY": account
            }
        }
    }
});

// 发送 DIFF 更新
let msg = DiffServerMessage::RtnData {
    data: vec![patch]
};
```

### 示例 2: 前端接收 DIFF 更新

```javascript
// web/src/utils/merge-patch.js
function mergePatch(target, patch) {
  if (typeof patch !== 'object' || patch === null || Array.isArray(patch)) {
    return patch
  }

  if (typeof target !== 'object' || target === null || Array.isArray(target)) {
    target = {}
  }

  for (const [key, value] of Object.entries(patch)) {
    if (value === null) {
      delete target[key]
    } else if (typeof value === 'object' && !Array.isArray(value)) {
      target[key] = mergePatch(target[key], value)
    } else {
      target[key] = value
    }
  }

  return target
}

// 应用 DIFF 更新
const snapshot = {}
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data)
  if (msg.aid === 'rtn_data') {
    for (const patch of msg.data) {
      mergePatch(snapshot, patch)
    }
    // 更新 Vuex
    store.commit('UPDATE_SNAPSHOT', snapshot)
  }
}
```
