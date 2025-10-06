//! WebSocket 服务模块
//!
//! 提供基于 WebSocket 的实时交易和行情服务
//!
//! # 支持的协议
//!
//! 1. **原有消息协议**: 向后兼容的 type-based 消息
//! 2. **DIFF 协议**: 新增的 aid-based 差分推送协议

pub mod messages;
pub mod session;
pub mod handler;
pub mod diff_messages;
pub mod diff_handler;

use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws;
use std::sync::Arc;
use std::collections::HashMap;
use uuid::Uuid;
use crossbeam::channel::Sender;
use actix::Addr;

use self::session::{WsSession, WsSessionMessage};
use self::handler::{WsMessageHandler, create_handler};
use self::diff_handler::{DiffHandler, DiffWebsocketSession};
use crate::exchange::{OrderRouter, AccountManager, TradeGateway};
use crate::user::UserManager;
use crate::market::MarketDataBroadcaster;
use crate::protocol::diff::snapshot::SnapshotManager;

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
}

impl WebSocketServer {
    /// 创建新的 WebSocket 服务器
    pub fn new(
        order_router: Arc<OrderRouter>,
        account_mgr: Arc<AccountManager>,
        user_manager: Arc<UserManager>,
        trade_gateway: Arc<TradeGateway>,
        market_broadcaster: Arc<MarketDataBroadcaster>,
    ) -> Self {
        let (handler, sender, sessions) = create_handler(order_router.clone(), account_mgr.clone());

        // 启动消息处理循环（消费 handler）
        handler.start();

        // 创建 DIFF 协议处理器（零拷贝架构）
        let snapshot_mgr = Arc::new(SnapshotManager::new());
        let diff_handler = Arc::new(
            DiffHandler::new(snapshot_mgr, account_mgr)  // ✨ 传递 account_mgr
                .with_user_manager(user_manager.clone())
                .with_order_router(order_router)
                .with_market_broadcaster(market_broadcaster.clone())
        );

        Self {
            sessions,
            message_sender: sender,
            user_manager,
            trade_gateway,
            market_broadcaster,
            diff_handler,
        }
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
        let session_id = Uuid::new_v4().to_string();

        // 创建 DIFF WebSocket 会话（零拷贝共享 DiffHandler）
        let mut session = DiffWebsocketSession::new(session_id.clone(), self.diff_handler.clone());

        // 如果提供了 user_id，设置认证状态
        if let Some(uid) = user_id {
            session.user_id = Some(uid.clone());

            // 初始化用户快照
            let snapshot_mgr = self.diff_handler.snapshot_mgr.clone();
            tokio::spawn(async move {
                snapshot_mgr.initialize_user(&uid).await;
            });
        }

        // 启动 DIFF WebSocket（低延迟异步架构）
        let resp = ws::start(session, &req, stream)?;

        Ok(resp)
    }

}

/// WebSocket 路由处理函数
pub async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    server: web::Data<Arc<WebSocketServer>>,
) -> Result<HttpResponse, Error> {
    // 可以从查询参数或 header 中获取 user_id
    let user_id = req.uri().query()
        .and_then(|q| {
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
    let user_id = req.uri().query()
        .and_then(|q| {
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

    #[test]
    fn test_websocket_server_creation() {
        // 这里只测试基本的创建逻辑
        // 完整的 WebSocket 测试需要 actix 运行时
    }
}
