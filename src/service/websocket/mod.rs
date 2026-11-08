//! WebSocket 服务模块
//!
//! 提供基于 WebSocket 的实时交易和行情服务
//!
//! # 支持的协议
//!
//! 1. **原有消息协议**: 向后兼容的 type-based 消息
//! 2. **DIFF 协议**: 新增的 aid-based 差分推送协议

pub mod diff_handler;
pub mod diff_messages;
pub mod handler;
pub mod messages;
pub mod session;

use actix::Addr;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use crossbeam::channel::Sender;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use self::diff_handler::{DiffHandler, DiffWebsocketSession};
use self::handler::{create_handler, WsMessageHandler};
use self::session::{WsSession, WsSessionMessage};
use crate::exchange::{AccountManager, OrderRouter, TradeGateway};
use crate::market::MarketDataBroadcaster;
use crate::protocol::diff::snapshot::SnapshotManager;
use crate::user::UserManager;

/// WebSocket 服务器
pub struct WebSocketServer {
    /// 会话映射（共享给 handler 和 sessions）
    sessions: Arc<parking_lot::RwLock<HashMap<String, Addr<WsSession>>>>,

    /// 消息发送器
    message_sender: Sender<WsSessionMessage>,

    /// 用户管理器 (用于JWT认证)
    user_manager: Arc<UserManager>,

    /// 成交回报网关
    trade_gateway: Arc<TradeGateway>,

    /// 市场数据广播器
    market_broadcaster: Arc<MarketDataBroadcaster>,

    /// DIFF 协议处理器（零拷贝共享）
    diff_handler: Arc<DiffHandler>,

    /// SnapshotManager - 用于广播公告等系统通知 @yutiansut @quantaxis
    snapshot_mgr: Arc<SnapshotManager>,
}

impl WebSocketServer {
    /// 创建新的 WebSocket 服务器
    pub fn new(
        order_router: Arc<OrderRouter>,
        account_mgr: Arc<AccountManager>,
        user_manager: Arc<UserManager>,
        trade_gateway: Arc<TradeGateway>,
        market_broadcaster: Arc<MarketDataBroadcaster>,
        kline_actor: actix::Addr<crate::market::KLineActor>,
        // ✨ 行情服务 —— 仅用于 DIFF 订阅时的初始快照。@yutiansut @quantaxis
        market_data_service: Arc<crate::market::MarketDataService>,
    ) -> Self {
        let (handler, sender, sessions) = create_handler(order_router.clone(), account_mgr.clone());

        // 启动消息处理循环（消费 handler）
        handler.start();

        // 创建 DIFF 协议处理器（零拷贝架构）
        let snapshot_mgr = Arc::new(SnapshotManager::new());

        // ✨ 关键修复：将 snapshot_mgr 设置到 trade_gateway，实现成交/订单实时推送 @yutiansut @quantaxis
        // 这样 TradeGateway 的 handle_filled/handle_cancel 等方法才能通过 SnapshotManager 推送数据
        // ✨ 不再需要 unsafe —— `snapshot_mgr` 已是 `OnceLock`,`set` 只要 `&self`。
        // 原写法 `Arc::as_ptr(&g) as *mut TradeGateway` 是 UB:`Arc<T>` 只给共享访问,
        // 而此时 gateway 早已被 clone(`main.rs:364` 交给了 OrderRouter),
        // 且该字段的读取点全在成交/撤单热路径上。@yutiansut @quantaxis
        trade_gateway.set_snapshot_manager(snapshot_mgr.clone());
        log::info!("✅ SnapshotManager connected to TradeGateway for real-time order/trade push");

        // ✨ 设置全局 SnapshotManager，用于系统公告广播 @yutiansut @quantaxis
        crate::service::http::account_admin::set_global_snapshot_manager(snapshot_mgr.clone());

        let diff_handler = Arc::new(
            DiffHandler::new(snapshot_mgr.clone(), account_mgr) // ✨ 传递 account_mgr，clone snapshot_mgr
                .with_user_manager(user_manager.clone())
                .with_order_router(order_router)
                .with_market_broadcaster(market_broadcaster.clone())
                // ✨ 用于 DIFF 订阅时补初始行情快照(见 diff_handler 里的说明)
                .with_market_data_service(market_data_service)
                .with_kline_actor(kline_actor), // ✨ 传递 K线Actor
        );

        Self {
            sessions,
            message_sender: sender,
            user_manager,
            trade_gateway,
            market_broadcaster,
            diff_handler,
            snapshot_mgr,
        }
    }

    /// 获取 SnapshotManager 用于广播系统通知 @yutiansut @quantaxis
    pub fn get_snapshot_manager(&self) -> Arc<SnapshotManager> {
        self.snapshot_mgr.clone()
    }

    /// 处理 WebSocket 连接
    pub async fn handle_connection(
        &self,
        req: HttpRequest,
        stream: web::Payload,
        user_id: Option<String>,
    ) -> Result<HttpResponse, Error> {
        let session_id = Uuid::new_v4().to_string();

        // 创建会话并设置 sessions 引用
        let mut session = WsSession::new(session_id.clone(), self.message_sender.clone())
            .with_sessions(self.sessions.clone())
            .with_user_manager(self.user_manager.clone())
            .with_market_broadcaster(self.market_broadcaster.clone());

        // 如果提供了 user_id，订阅成交通知
        if let Some(ref uid) = user_id {
            let receiver = self.trade_gateway.subscribe_user(uid.clone());
            session.notification_receiver = Some(receiver);
        }

        // 启动 WebSocket（session 会在 Actor::started() 中自动注册）
        let resp = ws::start(session, &req, stream)?;

        Ok(resp)
    }

    /// 处理 DIFF 协议 WebSocket 连接
    ///
    /// 路由: `/ws/diff?user_id=<user_id>`
    ///
    /// # 性能特点
    ///
    /// - **零拷贝**: Arc<DiffHandler> 共享，无数据克隆
    /// - **低延迟**: peek_message 使用 Tokio Notify，零轮询
    /// - **高并发**: 支持万级用户同时连接
    pub async fn handle_diff_connection(
        &self,
        req: HttpRequest,
        stream: web::Payload,
        user_id: Option<String>,
    ) -> Result<HttpResponse, Error> {
        // ✨ 缺 user_id 直接拒绝,不要让它「连上却收不到任何东西」。@yutiansut @quantaxis
        //
        // 原来 user_id 是 Option,缺失时只是跳过 `initialize_user`,
        // 但**握手照样返回 101**。实测(2026-09-06)不带 `?user_id=` 连接:
        //   握手 → `HTTP/1.1 101 Switching Protocols`(看起来成功)
        //   ping → **无 pong**
        //   subscribe_quote + peek_message → **0 条消息**
        // 带上 user_id 后同一探针立刻收到初始快照 + 增量行情 + factors/klines。
        //
        // 这个失败模式极具迷惑性:连接「成功」了,客户端会一直等下去,
        // 既没有错误码也没有关闭帧,排查会往行情/订阅/序列化各个方向找。
        // 明确拒绝比静默不工作好。
        let user_id = match user_id {
            Some(uid) if !uid.trim().is_empty() => uid,
            _ => {
                log::warn!("DIFF WebSocket 连接缺少 user_id 查询参数,已拒绝");
                return Ok(HttpResponse::BadRequest()
                    .body("missing required query parameter: user_id (use /ws/diff?user_id=<ID>)"));
            }
        };

        let session_id = Uuid::new_v4().to_string();

        // 创建 DIFF WebSocket 会话（零拷贝共享 DiffHandler）
        let mut session = DiffWebsocketSession::new(session_id.clone(), self.diff_handler.clone());

        // user_id 在上面已保证非空。连接截面在 `DiffWebsocketSession::started()` 里登记
        // (每条连接一份,绑定 `?user_id=` 主题)—— 不再按 user_id 共用一份。@yutiansut @quantaxis
        session.user_id = Some(user_id);

        // 启动 DIFF WebSocket（低延迟异步架构）
        let resp = ws::start(session, &req, stream)?;

        Ok(resp)
    }
}

/// WebSocket 路由处理函数
// ==================== WebSocket 连接计数 ====================
// @yutiansut @quantaxis
//
// ⚠️ 为什么是全局静态而不是放 AppState:
// WS 服务器(8095)在 main.rs:868 建的是**独立的 App**,只注入了 ws_server,
// 拿不到 HTTP 应用(8094)的 AppState —— 而 `AppState::ws_connection_count`
// 就住在那里。所以它被创建(main.rs:725)、被读取(monitoring.rs:503),
// 却**从来没有任何地方 +1/-1**,系统总览永远显示 0 连接
// (实测 8095 上 8 个 ESTABLISHED,接口报 0)。
//
// 跨 App 共享在本项目已有先例:`GLOBAL_SNAPSHOT_MANAGER`
// (account_admin.rs:49,同样是 WS 侧设置、HTTP 侧读取)。计数器是
// const 可构造的,连 OnceCell 都不需要。
static WS_CONNECTION_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// 会话建立时调用
pub fn ws_connection_opened() -> usize {
    WS_CONNECTION_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// 会话结束时调用
///
/// 用 CAS 循环而不是 `fetch_sub` —— usize 减到 0 以下会**回绕成天文数字**,
/// 界面会显示 18446744073709551615 连接。宁可少减,不可回绕。
pub fn ws_connection_closed() -> usize {
    use std::sync::atomic::Ordering;
    let mut cur = WS_CONNECTION_COUNT.load(Ordering::Relaxed);
    loop {
        if cur == 0 {
            log::warn!("ws_connection_closed 在计数为 0 时被调用(重复关闭?)");
            return 0;
        }
        match WS_CONNECTION_COUNT.compare_exchange_weak(
            cur, cur - 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return cur - 1,
            Err(actual) => cur = actual,
        }
    }
}

/// 当前活跃 WebSocket 连接数
pub fn ws_connection_count() -> usize {
    WS_CONNECTION_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}

pub async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    server: web::Data<Arc<WebSocketServer>>,
) -> Result<HttpResponse, Error> {
    // 可以从查询参数或 header 中获取 user_id
    let user_id = req.uri().query().and_then(|q| {
        q.split('&')
            .find(|s| s.starts_with("user_id="))
            .and_then(|s| s.strip_prefix("user_id="))
            .map(|s| s.to_string())
    });

    server.handle_connection(req, stream, user_id).await
}

/// DIFF 协议 WebSocket 路由处理函数
///
/// 路由: `/ws/diff?user_id=<user_id>`
///
/// # 性能优势
///
/// - **零拷贝**: 所有会话共享 Arc<DiffHandler>
/// - **零轮询**: peek_message 使用 Tokio Notify，无 CPU 浪费
/// - **高并发**: DashMap 支持万级并发用户
pub async fn ws_diff_route(
    req: HttpRequest,
    stream: web::Payload,
    server: web::Data<Arc<WebSocketServer>>,
) -> Result<HttpResponse, Error> {
    // 从查询参数获取 user_id
    let user_id = req.uri().query().and_then(|q| {
        q.split('&')
            .find(|s| s.starts_with("user_id="))
            .and_then(|s| s.strip_prefix("user_id="))
            .map(|s| s.to_string())
    });

    server.handle_diff_connection(req, stream, user_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ❌ 已删除空的 `test_websocket_server_creation` @yutiansut @quantaxis
    //
    // 它的函数体是**空的**,只有两行注释。空测试「通过但零覆盖」——
    // 在测试计数里占一格,让人以为这条路径有覆盖,比没有测试更坏。
    //
    // 与 `service/http/mod.rs` 那个不同,这里的理由**成立**:
    // `WebSocketServer::new` 需要 `actix::Addr<KLineActor>`,
    // 而 Addr 必须在 actix System 里创建 —— 不是「组装结构体」那么简单。
    // 要补就得写 `#[actix_web::test]` 并起一个 System,
    // 那已经是集成测试的范畴(`tests/notification_integration_test.rs`
    // 的 7 个 `#[tokio::test]` 就是这么做的)。
    //
    // 所以:删掉假信号,不假装有覆盖。真要补,应加在 tests/ 下。
}
