//! 业务快照管理器
//!
//! 管理业务快照的生命周期，实现 DIFF 协议的核心同步机制。
//!
//! # 核心功能
//!
//! - **快照管理**: 维护每个用户的业务快照（BusinessSnapshot）
//! - **差分推送**: 生成和应用 JSON Merge Patch
//! - **peek() 阻塞**: 实现 DIFF 协议的阻塞等待机制
//! - **并发访问**: 线程安全的多用户并发支持
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   SnapshotManager                            │
//! │  ┌────────────────────────────────────────────────────────┐ │
//! │  │ user_snapshots: DashMap<user_id, UserSnapshotState>   │ │
//! │  │   ├─ snapshot: BusinessSnapshot                        │ │
//! │  │   ├─ pending_patches: Vec<Value>                       │ │
//! │  │   └─ notifier: Arc<Notify>                             │ │
//! │  └────────────────────────────────────────────────────────┘ │
//! │                                                              │
//! │  Methods:                                                    │
//! │  - update_snapshot()  → 更新快照并生成 patch                │
//! │  - peek()             → 阻塞等待新 patch                     │
//! │  - apply_patches()    → 应用 patch 到快照                   │
//! │  - get_snapshot()     → 获取当前快照                        │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! # 使用示例
//!
//! ```rust
//! use qaexchange::protocol::diff::snapshot::SnapshotManager;
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() {
//!     let manager = SnapshotManager::new();
//!
//!     // 初始化用户快照
//!     manager.initialize_user("user123").await;
//!
//!     // 更新账户余额
//!     let patch = json!({
//!         "trade": {
//!             "user123": {
//!                 "accounts": {
//!                     "ACC001": {
//!                         "balance": 105000.0
//!                     }
//!                 }
//!             }
//!         }
//!     });
//!     manager.push_patch("user123", patch).await;
//!
//!     // 客户端 peek（阻塞直到有新数据）
//!     let patches = manager.peek("user123").await;
//!     println!("收到 {} 个 patch", patches.len());
//! }
//! ```

use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::timeout;

use super::merge::merge_patch;

/// 用户快照状态
#[derive(Debug)]
struct UserSnapshotState {
    /// 业务快照（完整状态）
    snapshot: parking_lot::RwLock<Value>,

    /// 待发送的 patch 队列
    pending_patches: parking_lot::RwLock<Vec<Value>>,

    /// 通知器（用于 peek 阻塞）
    notifier: Arc<Notify>,
}

impl UserSnapshotState {
    /// 创建新的用户快照状态
    fn new() -> Self {
        Self {
            snapshot: parking_lot::RwLock::new(Value::Object(serde_json::Map::new())),
            pending_patches: parking_lot::RwLock::new(Vec::new()),
            notifier: Arc::new(Notify::new()),
        }
    }

    /// 推送 patch 到待发送队列
    fn push_patch(&self, patch: Value) {
        let mut patches = self.pending_patches.write();
        patches.push(patch.clone());
        drop(patches); // 提前释放锁

        // 应用 patch 到快照
        let mut snapshot = self.snapshot.write();
        merge_patch(&mut snapshot, &patch);
        drop(snapshot);

        // 通知等待的客户端
        self.notifier.notify_waiters();
    }

    /// 获取所有待发送的 patch 并清空队列
    fn take_pending_patches(&self) -> Vec<Value> {
        let mut patches = self.pending_patches.write();
        std::mem::take(&mut *patches)
    }

    /// 检查是否有待发送的 patch
    fn has_pending_patches(&self) -> bool {
        !self.pending_patches.read().is_empty()
    }

    /// 获取当前快照的副本
    fn get_snapshot_clone(&self) -> Value {
        self.snapshot.read().clone()
    }
}

/// 业务快照管理器
///
/// 线程安全的快照管理器，支持多用户并发访问。
pub struct SnapshotManager {
    /// 用户快照映射 (user_id -> UserSnapshotState)
    user_snapshots: DashMap<String, Arc<UserSnapshotState>>,

    /// peek() 超时时间（默认 30 秒）
    peek_timeout: Duration,
}

impl SnapshotManager {
    /// 创建新的快照管理器
    ///
    /// # 示例
    ///
    /// ```rust
    /// use qaexchange::protocol::diff::snapshot::SnapshotManager;
    ///
    /// let manager = SnapshotManager::new();
    /// ```
    pub fn new() -> Self {
        Self {
            user_snapshots: DashMap::new(),
            peek_timeout: Duration::from_secs(30),
        }
    }

    /// 使用自定义 peek 超时时间创建管理器
    ///
    /// # 参数
    ///
    /// * `peek_timeout` - peek() 阻塞超时时间
    ///
    /// # 示例
    ///
    /// ```rust
    /// use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// use std::time::Duration;
    ///
    /// let manager = SnapshotManager::with_timeout(Duration::from_secs(60));
    /// ```
    pub fn with_timeout(peek_timeout: Duration) -> Self {
        Self {
            user_snapshots: DashMap::new(),
            peek_timeout,
        }
    }

    /// 初始化用户快照
    ///
    /// 为新用户创建空的业务快照。
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户ID
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    /// # }
    /// ```
    pub async fn initialize_user(&self, user_id: &str) {
        self.user_snapshots
            .entry(user_id.to_string())
            .or_insert_with(|| Arc::new(UserSnapshotState::new()));
    }

    /// 推送 patch 到用户快照
    ///
    /// 将 patch 添加到用户的待发送队列，并应用到快照。
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户ID
    /// * `patch` - JSON Merge Patch 对象
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    ///
    /// let patch = json!({
    ///     "trade": {
    ///         "user123": {
    ///             "accounts": {
    ///                 "ACC001": {"balance": 105000.0}
    ///             }
    ///         }
    ///     }
    /// });
    /// manager.push_patch("user123", patch).await;
    /// # }
    /// ```
    pub async fn push_patch(&self, user_id: &str, patch: Value) {
        let state = self
            .user_snapshots
            .entry(user_id.to_string())
            .or_insert_with(|| Arc::new(UserSnapshotState::new()))
            .clone();

        state.push_patch(patch);
    }

    /// peek() 阻塞等待新 patch
    ///
    /// 实现 DIFF 协议的核心同步机制：
    /// 1. 如果有待发送的 patch，立即返回
    /// 2. 否则，阻塞等待直到有新 patch 或超时
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户ID
    ///
    /// # 返回
    ///
    /// `Some(patches)` - 待发送的 patch 数组
    /// `None` - 超时或用户不存在
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    ///
    /// // 客户端调用 peek_message（阻塞等待）
    /// if let Some(patches) = manager.peek("user123").await {
    ///     println!("收到 {} 个 patch", patches.len());
    /// }
    /// # }
    /// ```
    pub async fn peek(&self, user_id: &str) -> Option<Vec<Value>> {
        let state = self.user_snapshots.get(user_id)?.clone();

        // 快速路径：如果已有 patch，立即返回
        if state.has_pending_patches() {
            return Some(state.take_pending_patches());
        }

        // 慢速路径：阻塞等待新 patch
        let notifier = state.notifier.clone();
        drop(state); // 提前释放 DashMap entry

        match timeout(self.peek_timeout, notifier.notified()).await {
            Ok(_) => {
                // 有新 patch，取出并返回
                self.user_snapshots
                    .get(user_id)
                    .map(|state| state.take_pending_patches())
            }
            Err(_) => {
                // 超时
                None
            }
        }
    }

    /// 获取用户当前快照
    ///
    /// 返回用户当前业务快照的副本。
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户ID
    ///
    /// # 返回
    ///
    /// `Some(snapshot)` - 业务快照
    /// `None` - 用户不存在
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    ///
    /// let snapshot = manager.get_snapshot("user123").await;
    /// println!("{:#?}", snapshot);
    /// # }
    /// ```
    pub async fn get_snapshot(&self, user_id: &str) -> Option<Value> {
        self.user_snapshots
            .get(user_id)
            .map(|state| state.get_snapshot_clone())
    }

    /// 批量应用 patch 到用户快照
    ///
    /// 按顺序应用多个 patch 到用户快照（不推送到待发送队列）。
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户ID
    /// * `patches` - patch 数组
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    ///
    /// let patches = vec![
    ///     json!({"trade": {"user123": {"accounts": {"ACC001": {"balance": 105000.0}}}}}),
    ///     json!({"trade": {"user123": {"accounts": {"ACC001": {"available": 100000.0}}}}}),
    /// ];
    /// manager.apply_patches("user123", patches).await;
    /// # }
    /// ```
    pub async fn apply_patches(&self, user_id: &str, patches: Vec<Value>) {
        if let Some(state) = self.user_snapshots.get(user_id) {
            let mut snapshot = state.snapshot.write();
            for patch in patches {
                merge_patch(&mut snapshot, &patch);
            }
        }
    }

    /// 移除用户快照
    ///
    /// 删除用户的业务快照（用于用户登出或清理）。
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户ID
    pub async fn remove_user(&self, user_id: &str) {
        self.user_snapshots.remove(user_id);
    }

    /// 获取当前用户数量
    pub fn user_count(&self) -> usize {
        self.user_snapshots.len()
    }

    /// 获取所有用户ID
    pub fn list_users(&self) -> Vec<String> {
        self.user_snapshots
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// 广播 patch 到所有已连接用户
    ///
    /// 将 patch 推送到所有已初始化的用户快照。
    /// 用于推送系统级通知（如公告、维护提醒等）。
    ///
    /// # 参数
    ///
    /// * `patch` - JSON Merge Patch 对象
    ///
    /// # 返回
    ///
    /// 成功推送的用户数量
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user1").await;
    /// manager.initialize_user("user2").await;
    ///
    /// // 广播公告通知
    /// let patch = json!({
    ///     "notify": {
    ///         "announcement_123": {
    ///             "type": "ANNOUNCEMENT",
    ///             "level": "INFO",
    ///             "code": 2000,
    ///             "title": "系统维护通知",
    ///             "content": "系统将于今晚22:00进行维护"
    ///         }
    ///     }
    /// });
    /// let count = manager.broadcast_patch(patch).await;
    /// println!("已推送到 {} 个用户", count);
    /// # }
    /// ```
    ///
    /// @yutiansut @quantaxis - 系统公告广播功能
    pub async fn broadcast_patch(&self, patch: Value) -> usize {
        let mut count = 0;
        for entry in self.user_snapshots.iter() {
            entry.value().push_patch(patch.clone());
            count += 1;
        }
        count
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_snapshot_manager_basic() {
        let manager = SnapshotManager::new();

        // 初始化用户
        manager.initialize_user("user123").await;

        // 推送 patch
        let patch1 = json!({"balance": 100000.0});
        manager.push_patch("user123", patch1.clone()).await;

        // 获取快照
        let snapshot = manager.get_snapshot("user123").await.unwrap();
        assert_eq!(snapshot["balance"], 100000.0);

        // peek 应该立即返回
        let patches = manager.peek("user123").await.unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0], patch1);
    }

    #[tokio::test]
    async fn test_peek_blocking() {
        let manager = Arc::new(SnapshotManager::with_timeout(Duration::from_secs(2)));
        manager.initialize_user("user123").await;

        let manager_clone = manager.clone();
        let peek_task = tokio::spawn(async move {
            // peek 应该阻塞直到有新 patch
            manager_clone.peek("user123").await
        });

        // 等待 100ms 后推送 patch
        sleep(Duration::from_millis(100)).await;
        let patch = json!({"balance": 105000.0});
        manager.push_patch("user123", patch.clone()).await;

        // peek 应该收到 patch
        let result = peek_task.await.unwrap();
        assert!(result.is_some());
        let patches = result.unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0], patch);
    }

    #[tokio::test]
    async fn test_peek_timeout() {
        let manager = SnapshotManager::with_timeout(Duration::from_millis(500));
        manager.initialize_user("user123").await;

        // peek 应该在 500ms 后超时
        let result = manager.peek("user123").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_multiple_patches() {
        let manager = SnapshotManager::new();
        manager.initialize_user("user123").await;

        // 推送多个 patch
        manager.push_patch("user123", json!({"a": 1})).await;
        manager.push_patch("user123", json!({"b": 2})).await;
        manager.push_patch("user123", json!({"c": 3})).await;

        // peek 应该返回所有 patch
        let patches = manager.peek("user123").await.unwrap();
        assert_eq!(patches.len(), 3);

        // 快照应该包含所有更新
        let snapshot = manager.get_snapshot("user123").await.unwrap();
        assert_eq!(snapshot["a"], 1);
        assert_eq!(snapshot["b"], 2);
        assert_eq!(snapshot["c"], 3);
    }

    #[tokio::test]
    async fn test_apply_patches() {
        let manager = SnapshotManager::new();
        manager.initialize_user("user123").await;

        // 批量应用 patch（不推送到待发送队列）
        let patches = vec![json!({"balance": 100000.0}), json!({"available": 95000.0})];
        manager.apply_patches("user123", patches).await;

        // 快照应该更新
        let snapshot = manager.get_snapshot("user123").await.unwrap();
        assert_eq!(snapshot["balance"], 100000.0);
        assert_eq!(snapshot["available"], 95000.0);

        // peek 应该超时（没有推送到待发送队列）
        let manager_timeout = SnapshotManager::with_timeout(Duration::from_millis(100));
        manager_timeout.initialize_user("user123").await;
        manager_timeout
            .apply_patches("user123", vec![json!({"x": 1})])
            .await;
        let result = manager_timeout.peek("user123").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_users() {
        let manager = Arc::new(SnapshotManager::new());

        // 初始化 10 个用户
        for i in 0..10 {
            manager.initialize_user(&format!("user{}", i)).await;
        }

        // 并发推送 patch
        let mut tasks = vec![];
        for i in 0..10 {
            let manager_clone = manager.clone();
            let task = tokio::spawn(async move {
                let user_id = format!("user{}", i);
                manager_clone
                    .push_patch(&user_id, json!({"user_num": i}))
                    .await;
            });
            tasks.push(task);
        }

        // 等待所有任务完成
        for task in tasks {
            task.await.unwrap();
        }

        // 验证每个用户的快照
        for i in 0..10 {
            let snapshot = manager.get_snapshot(&format!("user{}", i)).await.unwrap();
            assert_eq!(snapshot["user_num"], i);
        }
    }

    #[tokio::test]
    async fn test_remove_user() {
        let manager = SnapshotManager::new();
        manager.initialize_user("user123").await;
        manager
            .push_patch("user123", json!({"balance": 100000.0}))
            .await;

        // 移除用户
        manager.remove_user("user123").await;

        // 获取快照应该返回 None
        let snapshot = manager.get_snapshot("user123").await;
        assert!(snapshot.is_none());
    }

    #[tokio::test]
    async fn test_user_count_and_list() {
        let manager = SnapshotManager::new();

        // 初始化 3 个用户
        manager.initialize_user("alice").await;
        manager.initialize_user("bob").await;
        manager.initialize_user("charlie").await;

        assert_eq!(manager.user_count(), 3);

        let users = manager.list_users();
        assert_eq!(users.len(), 3);
        assert!(users.contains(&"alice".to_string()));
        assert!(users.contains(&"bob".to_string()));
        assert!(users.contains(&"charlie".to_string()));
    }

    #[tokio::test]
    async fn test_nested_object_merge() {
        let manager = SnapshotManager::new();
        manager.initialize_user("user123").await;

        // 推送嵌套对象 patch
        let patch1 = json!({
            "trade": {
                "user123": {
                    "accounts": {
                        "ACC001": {
                            "balance": 100000.0,
                            "available": 95000.0
                        }
                    }
                }
            }
        });
        manager.push_patch("user123", patch1).await;

        // 部分更新
        let patch2 = json!({
            "trade": {
                "user123": {
                    "accounts": {
                        "ACC001": {
                            "balance": 105000.0
                        }
                    }
                }
            }
        });
        manager.push_patch("user123", patch2).await;

        // 快照应该正确合并
        let snapshot = manager.get_snapshot("user123").await.unwrap();
        assert_eq!(
            snapshot["trade"]["user123"]["accounts"]["ACC001"]["balance"],
            105000.0
        );
        assert_eq!(
            snapshot["trade"]["user123"]["accounts"]["ACC001"]["available"],
            95000.0
        );
    }

    #[tokio::test]
    async fn test_high_frequency_updates() {
        let manager = Arc::new(SnapshotManager::new());
        manager.initialize_user("user123").await;

        let update_count = Arc::new(AtomicUsize::new(0));
        let update_count_clone = update_count.clone();

        // 高频更新任务
        let manager_clone = manager.clone();
        let update_task = tokio::spawn(async move {
            for i in 0..1000 {
                manager_clone
                    .push_patch("user123", json!({"counter": i}))
                    .await;
                update_count_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        update_task.await.unwrap();

        // 验证快照最终状态
        let snapshot = manager.get_snapshot("user123").await.unwrap();
        assert_eq!(snapshot["counter"], 999);
        assert_eq!(update_count.load(Ordering::SeqCst), 1000);
    }
}
