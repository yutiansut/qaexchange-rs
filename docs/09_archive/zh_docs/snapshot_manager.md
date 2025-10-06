# 业务快照管理器文档

## 概述

业务快照管理器 (SnapshotManager) 是 DIFF 协议的核心同步引擎，负责管理每个用户的业务快照，实现高效的差分推送机制。

## 核心概念

### 业务快照 (Business Snapshot)

业务快照是客户端和服务器共同维护的完整状态镜像，包含：

- **交易数据** (`trade`): 账户、持仓、委托单、成交记录
- **行情数据** (`quotes`): 实时行情、五档盘口
- **K线数据** (`klines`): 多周期K线图
- **Tick数据** (`ticks`): 逐笔成交数据
- **通知数据** (`notify`): 系统通知、错误消息

### 差分推送机制

```text
服务器端                                  客户端
   │                                        │
   ├──► push_patch(patch)                  │
   │    ├─ 添加到待发送队列                 │
   │    ├─ 应用到服务器快照                 │
   │    └─ 通知等待的客户端                 │
   │                                        │
   │    ◄──── peek()（阻塞等待）            │
   │                                        │
   ├──────── rtn_data(patches) ───────────►│
   │                                        ├─ 应用 patches 到客户端快照
   │                                        └─ 更新 UI
   │                                        │
```

### peek() 阻塞机制

DIFF 协议的核心同步机制，工作流程如下：

1. **客户端发起 peek_message 请求**
2. **服务器检查是否有待发送的 patch**
   - 有 patch → 立即返回 `rtn_data`
   - 无 patch → 阻塞等待（最长 30 秒）
3. **服务器有新数据时**
   - 唤醒所有等待的客户端
   - 返回 `rtn_data` 消息
4. **超时处理**
   - 30 秒后自动超时，客户端重新发起 peek

## 架构设计

### 数据结构

```rust
SnapshotManager
  └─ user_snapshots: DashMap<user_id, UserSnapshotState>
       └─ UserSnapshotState
            ├─ snapshot: RwLock<Value>           // 业务快照（完整状态）
            ├─ pending_patches: RwLock<Vec<Value>>  // 待发送 patch 队列
            └─ notifier: Arc<Notify>             // 异步通知器
```

### 线程安全

- **DashMap**: 无锁并发哈希表，支持多用户并发访问
- **RwLock**: 读写锁，允许多读单写
- **Arc<Notify>**: 异步通知机制，支持多个等待者

### 性能特点

| 操作 | 复杂度 | 说明 |
|------|--------|------|
| `initialize_user` | O(1) | DashMap 插入 |
| `push_patch` | O(n) | n = patch 大小，merge_patch 复杂度 |
| `peek` (快速路径) | O(1) | 队列非空，立即返回 |
| `peek` (慢速路径) | 阻塞 | 等待新 patch 或超时 |
| `get_snapshot` | O(m) | m = snapshot 大小，clone 操作 |
| `apply_patches` | O(k*n) | k = patch 数量，n = 单个 patch 大小 |

## API 文档

### `SnapshotManager::new()`

创建新的快照管理器（默认 peek 超时 30 秒）。

**示例**

```rust
use qaexchange::protocol::diff::snapshot::SnapshotManager;

let manager = SnapshotManager::new();
```

### `SnapshotManager::with_timeout(duration)`

使用自定义 peek 超时时间创建管理器。

**参数**
- `duration` - peek() 阻塞超时时间

**示例**

```rust
use qaexchange::protocol::diff::snapshot::SnapshotManager;
use std::time::Duration;

// 设置 60 秒超时
let manager = SnapshotManager::with_timeout(Duration::from_secs(60));
```

### `initialize_user(user_id)`

为新用户创建空的业务快照。

**参数**
- `user_id` - 用户ID

**示例**

```rust
manager.initialize_user("user123").await;
```

### `push_patch(user_id, patch)`

推送 patch 到用户快照，触发差分推送。

**参数**
- `user_id` - 用户ID
- `patch` - JSON Merge Patch 对象

**功能**
1. 添加 patch 到待发送队列
2. 应用 patch 到服务器快照
3. 通知所有等待的 peek() 调用

**示例**

```rust
use serde_json::json;

let patch = json!({
    "trade": {
        "user123": {
            "accounts": {
                "ACC001": {
                    "balance": 105000.0,
                    "available": 100000.0
                }
            }
        }
    }
});

manager.push_patch("user123", patch).await;
```

### `peek(user_id) -> Option<Vec<Value>>`

阻塞等待新 patch（DIFF 协议核心方法）。

**参数**
- `user_id` - 用户ID

**返回**
- `Some(patches)` - 待发送的 patch 数组
- `None` - 超时或用户不存在

**行为**
1. **快速路径**：如果有待发送的 patch，立即返回
2. **慢速路径**：阻塞等待直到有新 patch 或超时（默认 30 秒）

**示例**

```rust
// 客户端调用 peek_message（阻塞等待）
match manager.peek("user123").await {
    Some(patches) => {
        println!("收到 {} 个 patch", patches.len());
        // 发送 rtn_data 到客户端
    }
    None => {
        println!("超时或用户不存在");
    }
}
```

### `get_snapshot(user_id) -> Option<Value>`

获取用户当前业务快照的副本。

**参数**
- `user_id` - 用户ID

**返回**
- `Some(snapshot)` - 业务快照
- `None` - 用户不存在

**示例**

```rust
if let Some(snapshot) = manager.get_snapshot("user123").await {
    println!("快照: {:#?}", snapshot);
}
```

### `apply_patches(user_id, patches)`

批量应用 patch 到用户快照（不推送到待发送队列）。

**用途**
- 初始化快照
- 从 WAL 恢复快照
- 手动同步快照

**参数**
- `user_id` - 用户ID
- `patches` - patch 数组

**示例**

```rust
let patches = vec![
    json!({"balance": 100000.0}),
    json!({"available": 95000.0}),
];

manager.apply_patches("user123", patches).await;
```

### `remove_user(user_id)`

移除用户快照（用于用户登出或清理）。

**参数**
- `user_id` - 用户ID

**示例**

```rust
manager.remove_user("user123").await;
```

### `user_count() -> usize`

获取当前用户数量。

**示例**

```rust
let count = manager.user_count();
println!("当前在线用户: {}", count);
```

### `list_users() -> Vec<String>`

获取所有用户ID列表。

**示例**

```rust
let users = manager.list_users();
println!("用户列表: {:?}", users);
```

## 使用场景

### 1. 账户余额更新

```rust
use serde_json::json;

// 用户入金，账户余额增加
let patch = json!({
    "trade": {
        "user123": {
            "accounts": {
                "ACC001": {
                    "balance": 105000.0,
                    "available": 100000.0
                }
            }
        }
    }
});

manager.push_patch("user123", patch).await;

// 客户端 peek 收到更新
if let Some(patches) = manager.peek("user123").await {
    // 发送 rtn_data 到 WebSocket 客户端
    send_rtn_data(user_id, patches).await;
}
```

### 2. 订单状态变化

```rust
// 订单成交
let patch = json!({
    "trade": {
        "user123": {
            "orders": {
                "order_12345": {
                    "status": "FINISHED",
                    "volume_left": 0
                }
            }
        }
    }
});

manager.push_patch("user123", patch).await;
```

### 3. 删除已完成订单

```rust
// 订单完成后从快照中删除
let patch = json!({
    "trade": {
        "user123": {
            "orders": {
                "order_12345": null  // null 表示删除
            }
        }
    }
});

manager.push_patch("user123", patch).await;
```

### 4. 行情数据推送

```rust
// 推送最新行情
let patch = json!({
    "quotes": {
        "SHFE.cu2512": {
            "last_price": 75230.0,
            "bid_price1": 75220.0,
            "ask_price1": 75240.0,
            "volume": 123456
        }
    }
});

// 广播给所有订阅该合约的用户
for user_id in subscribed_users {
    manager.push_patch(&user_id, patch.clone()).await;
}
```

### 5. 初始化快照（用户登录）

```rust
// 用户登录，加载完整快照
let initial_snapshot = load_user_snapshot_from_db(user_id).await;

// 初始化用户
manager.initialize_user(user_id).await;

// 转换为 patch 格式并应用
let patches = vec![initial_snapshot];
manager.apply_patches(user_id, patches).await;
```

## 与 WebSocket 集成

### 服务器端处理流程

```rust
// WebSocket 会话处理
async fn handle_websocket_message(
    manager: Arc<SnapshotManager>,
    user_id: String,
    message: ClientMessage,
) {
    match message {
        ClientMessage::PeekMessage => {
            // 阻塞等待新 patch
            if let Some(patches) = manager.peek(&user_id).await {
                // 发送 rtn_data 到客户端
                let response = DiffServerMessage::RtnData { data: patches };
                send_to_client(response).await;
            }
        }
        ClientMessage::Subscribe { ins_list } => {
            // 订阅行情
            subscribe_quotes(user_id, ins_list).await;
        }
        // ... 其他消息处理
    }
}
```

### 业务逻辑集成

```rust
// AccountManager 集成
impl AccountManager {
    pub async fn deposit(
        &self,
        user_id: &str,
        amount: f64,
        snapshot_mgr: Arc<SnapshotManager>,
    ) -> Result<()> {
        // 1. 更新账户余额
        let account = self.get_account_mut(user_id)?;
        account.deposit(amount);

        // 2. 生成 patch
        let patch = json!({
            "trade": {
                user_id: {
                    "accounts": {
                        account.account_id(): {
                            "balance": account.balance,
                            "available": account.available
                        }
                    }
                }
            }
        });

        // 3. 推送 patch
        snapshot_mgr.push_patch(user_id, patch).await;

        Ok(())
    }
}
```

## 性能优化

### 1. 批量推送

减少 patch 数量，提高网络效率：

```rust
// ❌ 不推荐：多次推送
manager.push_patch(user_id, json!({"a": 1})).await;
manager.push_patch(user_id, json!({"b": 2})).await;
manager.push_patch(user_id, json!({"c": 3})).await;

// ✅ 推荐：合并为单个 patch
let patch = json!({
    "a": 1,
    "b": 2,
    "c": 3
});
manager.push_patch(user_id, patch).await;
```

### 2. 增量更新

只推送变化的字段：

```rust
// ❌ 不推荐：推送整个账户对象
let patch = json!({
    "trade": {
        "user123": {
            "accounts": {
                "ACC001": {
                    "user_id": "user123",
                    "currency": "CNY",
                    "balance": 105000.0,
                    "available": 100000.0,
                    "margin": 5000.0,
                    "frozen_margin": 0.0,
                    "risk_ratio": 0.05
                    // ... 20+ 字段
                }
            }
        }
    }
});

// ✅ 推荐：只推送变化字段
let patch = json!({
    "trade": {
        "user123": {
            "accounts": {
                "ACC001": {
                    "balance": 105000.0,
                    "available": 100000.0
                }
            }
        }
    }
});
```

### 3. 控制快照大小

定期清理历史数据：

```rust
// 删除已完成的订单（保留最近 100 条）
if finished_orders.len() > 100 {
    let to_delete = &finished_orders[100..];
    let mut patch_obj = serde_json::Map::new();

    for order_id in to_delete {
        patch_obj.insert(order_id.clone(), Value::Null);
    }

    let patch = json!({
        "trade": {
            user_id: {
                "orders": patch_obj
            }
        }
    });

    manager.push_patch(user_id, patch).await;
}
```

## 并发控制

### 多线程安全

SnapshotManager 完全线程安全，可在多个 async 任务中共享：

```rust
let manager = Arc::new(SnapshotManager::new());

// 并发推送 patch（多个业务模块）
let manager_clone1 = manager.clone();
tokio::spawn(async move {
    manager_clone1.push_patch("user123", json!({"a": 1})).await;
});

let manager_clone2 = manager.clone();
tokio::spawn(async move {
    manager_clone2.push_patch("user123", json!({"b": 2})).await;
});

// 并发 peek（多个 WebSocket 连接）
for i in 0..10 {
    let manager_clone = manager.clone();
    tokio::spawn(async move {
        if let Some(patches) = manager_clone.peek(&format!("user{}", i)).await {
            send_patches(patches).await;
        }
    });
}
```

### 顺序保证

同一用户的 patch 按推送顺序应用：

```rust
// 保证 patch1 在 patch2 之前应用
manager.push_patch(user_id, patch1).await;
manager.push_patch(user_id, patch2).await;

// peek 返回的 patches 顺序保持：[patch1, patch2]
let patches = manager.peek(user_id).await.unwrap();
```

## 故障排查

### 问题：peek() 一直超时

**可能原因**
- 没有推送 patch 到用户
- 用户ID不匹配
- 用户未初始化

**解决方法**

```rust
// 1. 检查用户是否初始化
let users = manager.list_users();
println!("当前用户: {:?}", users);

// 2. 手动推送测试 patch
manager.push_patch(user_id, json!({"test": 1})).await;

// 3. 检查 peek 超时设置
let manager = SnapshotManager::with_timeout(Duration::from_secs(60));
```

### 问题：快照数据不一致

**可能原因**
- patch 应用顺序错误
- 并发更新冲突
- patch 格式错误

**解决方法**

```rust
// 1. 验证快照内容
let snapshot = manager.get_snapshot(user_id).await.unwrap();
println!("快照: {:#?}", snapshot);

// 2. 重新初始化快照
manager.remove_user(user_id).await;
manager.initialize_user(user_id).await;
let full_snapshot = load_from_db(user_id).await;
manager.apply_patches(user_id, vec![full_snapshot]).await;
```

### 问题：内存占用过高

**可能原因**
- 快照过大（历史数据未清理）
- 用户数量过多
- 待发送队列积压

**解决方法**

```rust
// 1. 定期清理离线用户
for user_id in offline_users {
    manager.remove_user(&user_id).await;
}

// 2. 限制快照大小（删除历史数据）
// 3. 监控待发送队列长度
```

## 测试覆盖

所有 10 个单元测试通过，覆盖率 > 90%：

- ✅ `test_snapshot_manager_basic` - 基本功能
- ✅ `test_peek_blocking` - peek 阻塞等待
- ✅ `test_peek_timeout` - peek 超时
- ✅ `test_multiple_patches` - 多个 patch 处理
- ✅ `test_apply_patches` - 批量应用 patch
- ✅ `test_concurrent_users` - 并发用户
- ✅ `test_remove_user` - 移除用户
- ✅ `test_user_count_and_list` - 用户统计
- ✅ `test_nested_object_merge` - 嵌套对象合并
- ✅ `test_high_frequency_updates` - 高频更新（1000次）

## 下一步

- [DIFF 数据类型定义](./diff_types.md) - Quote, Kline, Notify 数据结构
- [WebSocket 集成指南](./websocket_integration.md) - WebSocket 消息处理
- [DIFF 协议完整文档](../DIFF_INTEGRATION.md) - DIFF 协议架构设计

## 参考资料

- [JSON Merge Patch 文档](./json_merge_patch.md)
- [DIFF 协议规范](../DIFF_INTEGRATION.md)
- [源代码](../../src/protocol/diff/snapshot.rs)
