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
//! │  │ conns:  DashMap<conn_id, ConnState>  (每连接一份截面)  │ │
//! │  │ topics: DashMap<user_id|account_id, Vec<conn>> (扇出)  │ │
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
//!     // 客户端 peek（阻塞直到有新数据）;参数是**连接 ID**(兼容接口里 = user_id)
//!     if let Some(patches) = manager.peek("user123").await {
//!         println!("收到 {} 个 patch", patches.len());
//!     }
//! }
//! ```

use dashmap::DashMap;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::timeout;

use super::merge::merge_patch;

/// **一条 DIFF 连接**的私有截面(DIFF / TqSdk 语义:每条 WebSocket 连接各自维护一份完整截面、
/// 各自 peek)。@yutiansut @quantaxis
///
/// ⚠️ 2026-11-08 之前截面是**按 user_id 一份**:同一用户开两条连接(hft_maker 行情一条、
/// 交易一条)时两条连接共用一个待发队列,`peek` 一次全部取走 —— 谁先 peek 谁拿走,
/// 登录通知、成交回报都可能被另一条连接「抢走」(OMS 收不到成交);
/// 任一条断开还会 `remove_user` 把共享截面整个删掉。现在每条连接一份,
/// 交易回报按**主题**(user_id / account_id)扇出给绑定了该主题的每条连接。
#[derive(Debug)]
struct ConnState {
    /// 本连接的业务截面(已发 + 待发补丁的合并结果)
    snapshot: parking_lot::RwLock<Value>,

    /// 待发送的 patch 队列
    pending_patches: parking_lot::RwLock<Vec<Value>>,

    /// 通知器(peek 阻塞等待)
    notifier: Notify,

    /// 本连接绑定的主题;`closed` 的判定与它在同一把锁内完成(见 `bind_topic`)
    topics: parking_lot::Mutex<Vec<String>>,

    /// 连接已注销:不再接受绑定
    closed: AtomicBool,
}

/// 无人消费时待发队列的上限。超过就折叠成**一张全量快照**:截面里本来就是合并后的完整状态,
/// 把它当作一个 patch 发出去,与新连接的初始同步等价。@yutiansut @quantaxis
///
/// 已知局限:折叠掉的补丁里若有 `null`(删除),客户端镜像里对应的旧键不会被删
/// (全量快照里没有这个键,合并时不会动它)。只有连续 2000 个补丁都不 peek 的连接才会走到这里。
const MAX_PENDING_PATCHES: usize = 2000;

impl ConnState {
    fn new() -> Self {
        Self {
            snapshot: parking_lot::RwLock::new(Value::Object(serde_json::Map::new())),
            pending_patches: parking_lot::RwLock::new(Vec::new()),
            notifier: Notify::new(),
            topics: parking_lot::Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
        }
    }

    /// 推送 patch 到待发送队列
    fn push_patch(&self, patch: Value) {
        // 先并入快照,再入队;队列超限就折叠成全量快照(锁序:snapshot → pending,
        // 其他路径都只单独持其中一把,不会死锁)@yutiansut @quantaxis
        let mut snapshot = self.snapshot.write();
        merge_patch(&mut snapshot, &patch);
        let mut patches = self.pending_patches.write();
        patches.push(patch);
        if patches.len() > MAX_PENDING_PATCHES {
            *patches = vec![snapshot.clone()];
        }
        drop(patches);
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
/// - **连接**(`conn_id`,即 WebSocket session_id):各自一份截面、各自 peek。
/// - **主题**(user_id / account_id):交易回报按主题推,扇出给绑定了该主题的每条连接;
///   一条连接可以绑定多个主题(按账户 ID 连接 + 登录为属主用户),同一个补丁只收一份。
/// - 会话私有的回包(登录结果、订阅确认、行情、K 线)用 `push_to_connection` 直推本连接。
///
/// 兼容旧接口:`initialize_user(id)` = 登记一条 `conn_id == topic == id` 的连接,
/// `peek` / `get_snapshot` / `remove_user` 的参数即连接 ID。
pub struct SnapshotManager {
    /// conn_id → 连接截面
    conns: DashMap<String, Arc<ConnState>>,

    /// 主题(user_id / account_id)→ 绑定的连接
    topics: DashMap<String, Vec<(String, Arc<ConnState>)>>,

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
        Self::with_timeout(Duration::from_secs(30))
    }

    /// 使用自定义 peek 超时时间创建管理器
    ///
    /// ```rust
    /// use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// use std::time::Duration;
    ///
    /// let manager = SnapshotManager::with_timeout(Duration::from_secs(60));
    /// ```
    pub fn with_timeout(peek_timeout: Duration) -> Self {
        Self {
            conns: DashMap::new(),
            topics: DashMap::new(),
            peek_timeout,
        }
    }

    /// 登记一条连接(幂等),`topic` 非空时同时绑定该主题。@yutiansut @quantaxis
    ///
    /// ```rust
    /// use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// let m = SnapshotManager::new();
    /// m.register_connection("session-1", "user123");
    /// m.register_connection("session-2", "user123"); // 同一用户第二条连接,各自一份截面
    /// assert!(m.has_subscribers("user123"));
    /// ```
    pub fn register_connection(&self, conn_id: &str, topic: &str) {
        self.conns
            .entry(conn_id.to_string())
            .or_insert_with(|| Arc::new(ConnState::new()));
        if !topic.is_empty() {
            self.bind_topic(conn_id, topic);
        }
    }

    /// 把连接绑定到主题(幂等)。连接不存在或已注销返回 `false`。
    ///
    /// 与 `remove_connection` 的竞态:二者都在连接的 `topics` 锁内判定 / 置位 `closed`,
    /// 绑定在锁内写入主题索引,所以注销看到的主题列表一定包含已完成的绑定,不会留下
    /// 「连接已注销、主题索引里还挂着它」的泄漏。
    pub fn bind_topic(&self, conn_id: &str, topic: &str) -> bool {
        let Some(conn) = self.conns.get(conn_id).map(|c| c.value().clone()) else {
            return false;
        };
        let mut bound = conn.topics.lock();
        if conn.closed.load(Ordering::Acquire) {
            return false;
        }
        if bound.iter().any(|t| t == topic) {
            return true;
        }
        bound.push(topic.to_string());
        self.topics
            .entry(topic.to_string())
            .or_default()
            .push((conn_id.to_string(), conn.clone()));
        true
    }

    /// 注销连接:解绑全部主题并删除其截面;挂着的 peek 会被唤醒。
    pub fn remove_connection(&self, conn_id: &str) {
        let Some((_, conn)) = self.conns.remove(conn_id) else {
            return;
        };
        let bound = {
            let mut t = conn.topics.lock();
            conn.closed.store(true, Ordering::Release);
            std::mem::take(&mut *t)
        };
        for topic in bound {
            if let Some(mut subs) = self.topics.get_mut(&topic) {
                subs.retain(|(id, _)| id != conn_id);
            } // 先放掉写锁,再 remove_if(同一分片,否则自锁)
            self.topics.remove_if(&topic, |_, subs| subs.is_empty());
        }
        conn.notifier.notify_waiters();
    }

    /// 兼容旧接口:登记 `conn_id == topic == user_id` 的连接
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
        self.register_connection(user_id, user_id);
    }

    /// 主题上有没有连接在听。没有就不必构造补丁 —— 之后连上的连接由登录初始快照补齐。
    pub fn has_subscribers(&self, topic: &str) -> bool {
        self.topics.get(topic).map_or(false, |subs| !subs.is_empty())
    }

    /// 把补丁推给绑定了 `topics` 中任一主题的连接,**每条连接只收一份**
    /// (连接可能同时绑定属主用户与账户两个主题)。返回送达的连接数。
    ///
    /// 同步、不持任何账户锁,可在持账户锁时调用;同一主题的补丁按调用顺序入队(FIFO)。
    pub fn push_patch_topics(&self, topics: &[&str], patch: Value) -> usize {
        let mut targets: Vec<Arc<ConnState>> = Vec::new();
        for topic in topics {
            if let Some(subs) = self.topics.get(*topic) {
                for (_, conn) in subs.iter() {
                    if !targets.iter().any(|t| Arc::ptr_eq(t, conn)) {
                        targets.push(conn.clone());
                    }
                }
            }
        }
        let n = targets.len();
        if let Some((last, rest)) = targets.split_last() {
            for conn in rest {
                conn.push_patch(patch.clone());
            }
            last.push_patch(patch);
        }
        n
    }

    /// 推给某个主题(user_id / account_id)的全部连接(同步)。@yutiansut @quantaxis
    ///
    /// 交易回报必须**按事件顺序**进截面:原先每个补丁 `tokio::spawn(push_patch)`,
    /// 同一账户的委托/成交补丁可能乱序到达客户端(hft_maker 提出)。在事件线程上
    /// 直接调本函数即可保证同账户 FIFO。不持有任何账户锁,可在持账户锁时调用。
    pub fn push_patch_now(&self, topic: &str, patch: Value) {
        self.push_patch_topics(&[topic], patch);
    }

    /// `push_patch_now` 的 async 版本(内部没有 await,保留给旧调用方)
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    /// manager.push_patch("user123", json!({"balance": 105000.0})).await;
    /// # }
    /// ```
    pub async fn push_patch(&self, topic: &str, patch: Value) {
        self.push_patch_now(topic, patch);
    }

    /// 只推给一条连接(登录结果、订阅确认、行情、K 线等会话私有的回包)。连接不存在返回 `false`。
    pub fn push_to_connection(&self, conn_id: &str, patch: Value) -> bool {
        let Some(conn) = self.conns.get(conn_id).map(|c| c.value().clone()) else {
            return false;
        };
        conn.push_patch(patch);
        true
    }

    /// peek() 阻塞等待本连接的新 patch
    ///
    /// 1. 有待发送的 patch,立即返回
    /// 2. 否则阻塞等待,直到有新 patch 或超时(`None`)
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    /// manager.push_patch("user123", json!({"a": 1})).await;
    /// if let Some(patches) = manager.peek("user123").await {
    ///     println!("收到 {} 个 patch", patches.len());
    /// }
    /// # }
    /// ```
    pub async fn peek(&self, conn_id: &str) -> Option<Vec<Value>> {
        let state = self.conns.get(conn_id).map(|c| c.value().clone())?;

        // ⚠️ 先建 `Notified` 再查队列。tokio 保证 `Notified` 一创建就能收到 `notify_waiters`;
        // 原先先查队列、后建 future,两步之间到达的补丁不会唤醒它 —— 只能等下一个补丁
        // 或 30 秒超时(丢唤醒;登录通知这种「之后再无补丁」的情形会卡满 30 秒)。
        // @yutiansut @quantaxis
        let notified = state.notifier.notified();
        if state.has_pending_patches() {
            return Some(state.take_pending_patches());
        }
        if state.closed.load(Ordering::Acquire) {
            return None;
        }

        match timeout(self.peek_timeout, notified).await {
            Ok(()) => Some(state.take_pending_patches()),
            Err(_) => None,
        }
    }

    /// 连接当前截面的副本;连接不存在返回 `None`
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    /// let snapshot = manager.get_snapshot("user123").await;
    /// println!("{:#?}", snapshot);
    /// # }
    /// ```
    pub async fn get_snapshot(&self, conn_id: &str) -> Option<Value> {
        self.conns.get(conn_id).map(|state| state.get_snapshot_clone())
    }

    /// 批量应用 patch 到连接截面(不进待发送队列)
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user123").await;
    /// manager.apply_patches("user123", vec![json!({"balance": 105000.0})]).await;
    /// # }
    /// ```
    pub async fn apply_patches(&self, conn_id: &str, patches: Vec<Value>) {
        if let Some(state) = self.conns.get(conn_id) {
            let mut snapshot = state.snapshot.write();
            for patch in patches {
                merge_patch(&mut snapshot, &patch);
            }
        }
    }

    /// 兼容旧接口 = `remove_connection`
    ///
    /// ⚠️ 本函数**不是** async —— 函数体只有 DashMap 操作,没有任何 await。
    /// 它原本被标成 `pub async fn`,逼得唯一的生产调用方
    /// (`diff_handler.rs` 的 `Actor::stopped()`)只能 `tokio::spawn` 一个
    /// 什么都不等的 future 来调它。而 `stopped()` 是同步生命周期钩子,
    /// 在 System 关停 / 跨线程 drop 时**所在线程没有 tokio reactor**,
    /// `tokio::spawn` 直接 panic:
    ///     there is no reactor running, must be called from the context of a Tokio 1.x runtime
    /// 一个 arbiter 线程炸掉,整个 HTTP 服务停止监听(进程还在,端口没了)。
    /// 2026-09-03 22:33:36 线上就是这么挂的。@yutiansut @quantaxis
    pub fn remove_user(&self, conn_id: &str) {
        self.remove_connection(conn_id);
    }

    /// 当前连接数
    pub fn user_count(&self) -> usize {
        self.conns.len()
    }

    /// 全部连接 ID
    pub fn list_users(&self) -> Vec<String> {
        self.conns.iter().map(|entry| entry.key().clone()).collect()
    }

    /// 广播 patch 到所有连接(系统公告、维护提醒等)。返回连接数。
    ///
    /// ```rust
    /// # use qaexchange::protocol::diff::snapshot::SnapshotManager;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let manager = SnapshotManager::new();
    /// manager.initialize_user("user1").await;
    /// manager.initialize_user("user2").await;
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
    /// assert_eq!(count, 2);
    /// # }
    /// ```
    ///
    /// @yutiansut @quantaxis - 系统公告广播功能
    pub async fn broadcast_patch(&self, patch: Value) -> usize {
        let mut count = 0;
        for entry in self.conns.iter() {
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
        manager.remove_user("user123");

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

/// ✨ 待发队列有界(2026-11-08)@yutiansut @quantaxis
#[cfg(test)]
mod bounded_pending_tests {
    use super::*;
    use serde_json::json;

    /// 连接一直不 peek 时队列超过上限要折叠成一张全量快照,且折叠后的快照与逐个合并的结果一致
    #[tokio::test]
    async fn pending_collapses_to_full_snapshot_without_consumer() {
        let sm = SnapshotManager::new();
        sm.register_connection("c", "u");
        for i in 0..(MAX_PENDING_PATCHES + 10) {
            sm.push_patch_now("u", json!({ "k": { format!("{}", i % 7): i } }));
        }
        let patches = sm.peek("c").await.expect("patches");
        assert!(patches.len() <= 11, "队列应被折叠,实际 {} 条", patches.len());
        // 把发出的补丁依次合并,结果必须等于服务端截面
        let mut client = json!({});
        for p in &patches {
            merge_patch(&mut client, p);
        }
        assert_eq!(client, sm.get_snapshot("c").await.unwrap());
    }

    /// 主题上没有连接时推送是空操作,不会凭空建出一份没人读的截面
    #[test]
    fn push_without_subscribers_allocates_nothing() {
        let sm = SnapshotManager::new();
        assert!(!sm.has_subscribers("u"));
        sm.push_patch_now("u", json!({"a": 1}));
        assert_eq!(sm.user_count(), 0);
        assert!(!sm.has_subscribers("u"));
    }
}

/// ✨ 按连接扇出(hft_maker 2026-11-08 06:18 复现:同一 user_id 两条连接互抢补丁)@yutiansut @quantaxis
#[cfg(test)]
mod per_connection_tests {
    use super::*;
    use serde_json::json;

    /// 同一用户两条连接:主题补丁两条都收到,各自 peek 互不影响
    #[tokio::test]
    async fn two_connections_of_one_user_each_get_every_patch() {
        let sm = SnapshotManager::with_timeout(Duration::from_millis(200));
        sm.register_connection("md", "hftm");
        sm.register_connection("td", "hftm");
        sm.push_patch_now("hftm", json!({"trade": {"A": {"trades": {"o1|1": {"volume": 1}}}}}));
        let md = sm.peek("md").await.expect("md");
        let td = sm.peek("td").await.expect("td");
        assert_eq!(md.len(), 1);
        assert_eq!(td.len(), 1);
        assert_eq!(md, td);
    }

    /// 两条连接都挂着 peek 时推一个补丁:两条都被唤醒且都拿到(原先一条拿走、另一条拿到空)
    #[tokio::test]
    async fn concurrent_peeks_on_two_connections_both_receive() {
        let sm = Arc::new(SnapshotManager::with_timeout(Duration::from_secs(5)));
        sm.register_connection("md", "hftm");
        sm.register_connection("td", "hftm");
        let a = tokio::spawn({ let sm = sm.clone(); async move { sm.peek("md").await } });
        let b = tokio::spawn({ let sm = sm.clone(); async move { sm.peek("td").await } });
        tokio::time::sleep(Duration::from_millis(50)).await;
        sm.push_patch_now("hftm", json!({"x": 1}));
        let a = a.await.unwrap().expect("md woke");
        let b = b.await.unwrap().expect("td woke");
        assert_eq!(a, vec![json!({"x": 1})]);
        assert_eq!(b, vec![json!({"x": 1})]);
    }

    /// 会话私有回包(登录结果等)只到发起的那条连接
    #[tokio::test]
    async fn direct_push_reaches_only_that_connection() {
        let sm = SnapshotManager::with_timeout(Duration::from_millis(100));
        sm.register_connection("md", "hftm");
        sm.register_connection("td", "hftm");
        assert!(sm.push_to_connection("md", json!({"notify": {"login_success": {}}})));
        assert_eq!(sm.peek("md").await.expect("md").len(), 1);
        assert!(sm.peek("td").await.is_none(), "td 不应收到 md 的登录回包");
        assert!(!sm.push_to_connection("nope", json!({})));
    }

    /// 断开一条连接不影响同用户的另一条(原先 remove_user 会把共享截面整个删掉)
    #[tokio::test]
    async fn removing_one_connection_keeps_the_other() {
        let sm = SnapshotManager::with_timeout(Duration::from_millis(100));
        sm.register_connection("md", "hftm");
        sm.register_connection("td", "hftm");
        sm.remove_connection("md");
        assert!(sm.has_subscribers("hftm"));
        sm.push_patch_now("hftm", json!({"y": 2}));
        assert_eq!(sm.peek("td").await.expect("td"), vec![json!({"y": 2})]);
        assert!(sm.peek("md").await.is_none());
        sm.remove_connection("td");
        assert!(!sm.has_subscribers("hftm"), "最后一条连接注销后主题应被清掉");
        assert_eq!(sm.user_count(), 0);
    }

    /// 同时绑定属主用户与账户两个主题的连接,同一补丁只收一份
    #[tokio::test]
    async fn connection_bound_to_owner_and_account_gets_one_copy() {
        let sm = SnapshotManager::with_timeout(Duration::from_millis(100));
        sm.register_connection("c", "ACC_1");
        assert!(sm.bind_topic("c", "owner"));
        assert!(sm.bind_topic("c", "owner"), "重复绑定幂等");
        let n = sm.push_patch_topics(&["owner", "ACC_1"], json!({"z": 3}));
        assert_eq!(n, 1);
        assert_eq!(sm.peek("c").await.expect("c").len(), 1);
    }

    /// 注销后的绑定被拒绝,不会在主题索引里留下挂着死连接的条目
    #[test]
    fn bind_after_remove_is_refused() {
        let sm = SnapshotManager::new();
        sm.register_connection("c", "u");
        sm.remove_connection("c");
        assert!(!sm.bind_topic("c", "v"));
        assert!(!sm.has_subscribers("v"));
        assert!(!sm.has_subscribers("u"));
    }

    /// 注销连接会唤醒它挂着的 peek(不必等满超时)
    #[tokio::test]
    async fn remove_wakes_pending_peek() {
        let sm = Arc::new(SnapshotManager::with_timeout(Duration::from_secs(10)));
        sm.register_connection("c", "u");
        let t0 = std::time::Instant::now();
        let h = tokio::spawn({ let sm = sm.clone(); async move { sm.peek("c").await } });
        tokio::time::sleep(Duration::from_millis(30)).await;
        sm.remove_connection("c");
        let r = h.await.unwrap();
        assert!(r.map_or(true, |v| v.is_empty()));
        assert!(t0.elapsed() < Duration::from_secs(2));
    }
}
