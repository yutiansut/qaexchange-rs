//! WebSocket 会话管理

use super::messages::{ClientMessage, ServerMessage};
use crate::exchange::TradeGateway;
use crate::market::{MarketDataBroadcaster, MarketDataEvent};
use crate::user::UserManager;
use actix::{Actor, ActorContext, Addr, AsyncContext, Handler, Message, StreamHandler};
use actix_web_actors::ws;
use crossbeam::channel::{Receiver, Sender};
use log;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// 客户端超时时间
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// WebSocket 会话状态
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    /// 未认证
    Unauthenticated,
    /// 已认证
    Authenticated { user_id: String },
}

/// WebSocket 会话
pub struct WsSession {
    /// 会话 ID
    pub id: String,

    /// 会话状态
    pub state: SessionState,

    /// 最后心跳时间
    pub heartbeat: Instant,

    /// 订阅的频道
    pub subscribed_channels: Vec<String>,

    /// 订阅的合约
    pub subscribed_instruments: Vec<String>,

    /// 成交通知接收器（来自 TradeGateway）
    pub notification_receiver: Option<Receiver<crate::exchange::Notification>>,

    /// 消息发送器（发送到业务逻辑处理器）
    pub message_sender: Sender<WsSessionMessage>,

    /// 会话映射引用（用于注册/注销自己）
    pub sessions: Option<Arc<parking_lot::RwLock<HashMap<String, Addr<WsSession>>>>>,

    /// 用户管理器 (用于JWT认证)
    pub user_manager: Option<Arc<UserManager>>,

    /// 市场数据广播器
    pub market_broadcaster: Option<Arc<MarketDataBroadcaster>>,

    /// 市场数据接收器
    pub market_data_receiver: Option<Receiver<MarketDataEvent>>,
}

/// 会话消息（发送给业务逻辑处理器）
#[derive(Debug, Clone)]
pub struct WsSessionMessage {
    pub session_id: String,
    pub user_id: Option<String>,
    pub message: ClientMessage,
}

impl WsSession {
    pub fn new(session_id: String, message_sender: Sender<WsSessionMessage>) -> Self {
        Self {
            id: session_id,
            state: SessionState::Unauthenticated,
            heartbeat: Instant::now(),
            subscribed_channels: Vec::new(),
            subscribed_instruments: Vec::new(),
            notification_receiver: None,
            message_sender,
            sessions: None,
            user_manager: None,
            market_broadcaster: None,
            market_data_receiver: None,
        }
    }

    /// 设置会话映射引用（用于注册）
    pub fn with_sessions(
        mut self,
        sessions: Arc<parking_lot::RwLock<HashMap<String, Addr<WsSession>>>>,
    ) -> Self {
        self.sessions = Some(sessions);
        self
    }

    /// 设置用户管理器 (用于JWT认证)
    pub fn with_user_manager(mut self, user_manager: Arc<UserManager>) -> Self {
        self.user_manager = Some(user_manager);
        self
    }

    /// 设置市场数据广播器
    pub fn with_market_broadcaster(mut self, broadcaster: Arc<MarketDataBroadcaster>) -> Self {
        self.market_broadcaster = Some(broadcaster);
        self
    }

    /// 启动心跳检查
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // 检查客户端是否超时
            if Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
                log::warn!("WebSocket session {} timed out, disconnecting", act.id);
                ctx.stop();
                return;
            }

            // 发送 ping
            ctx.ping(b"");
        });
    }

    /// 启动市场数据监听
    fn start_market_data_listener(&self, ctx: &mut ws::WebsocketContext<Self>) {
        if let Some(ref receiver) = self.market_data_receiver {
            let receiver = receiver.clone();
            let mut dropped_count = 0u64;
            let mut last_warn_time = std::time::Instant::now();

            ctx.run_interval(Duration::from_millis(10), move |_act, ctx| {
                // 批量接收市场数据事件
                let mut events = Vec::new();
                let max_batch_size = 100;

                // 检测背压：如果队列中有大量积压事件，优先丢弃旧事件
                let queue_len = receiver.len();
                if queue_len > 500 {
                    // 背压触发：丢弃一半旧事件
                    let to_drop = queue_len / 2;
                    for _ in 0..to_drop {
                        if receiver.try_recv().is_ok() {
                            dropped_count += 1;
                        }
                    }

                    // 每5秒最多警告一次
                    if last_warn_time.elapsed() > Duration::from_secs(5) {
                        log::warn!(
                            "WebSocket backpressure: queue_len={}, dropped {} events (total: {})",
                            queue_len,
                            to_drop,
                            dropped_count
                        );
                        last_warn_time = std::time::Instant::now();
                    }
                }

                // 批量接收事件
                while let Ok(event) = receiver.try_recv() {
                    events.push(event);
                    if events.len() >= max_batch_size {
                        break;
                    }
                }

                // 批量发送：合并为JSON数组，一次性发送
                if !events.is_empty() {
                    match serde_json::to_string(&events) {
                        Ok(batch_json) => {
                            ctx.text(batch_json);
                        }
                        Err(e) => {
                            log::error!("Failed to serialize market data batch: {}", e);
                        }
                    }
                }
            });
        }
    }

    /// 处理客户端消息
    fn handle_client_message(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        match &msg {
            ClientMessage::Auth { user_id, token } => {
                // 使用 JWT 验证 token
                if let Some(ref user_mgr) = self.user_manager {
                    match user_mgr.verify_token(token) {
                        Ok(verified_user_id) => {
                            // Token 验证成功
                            self.state = SessionState::Authenticated {
                                user_id: verified_user_id.clone(),
                            };

                            let response = ServerMessage::AuthResponse {
                                success: true,
                                user_id: verified_user_id.clone(),
                                message: "Authentication successful".to_string(),
                            };

                            if let Ok(json) = serde_json::to_string(&response) {
                                ctx.text(json);
                            }

                            log::info!(
                                "Session {} authenticated as user {} via JWT",
                                self.id,
                                verified_user_id
                            );
                        }
                        Err(e) => {
                            // Token 验证失败
                            let response = ServerMessage::AuthResponse {
                                success: false,
                                user_id: String::new(),
                                message: format!("Authentication failed: {}", e),
                            };

                            if let Ok(json) = serde_json::to_string(&response) {
                                ctx.text(json);
                            }

                            log::warn!("Session {} authentication failed: {}", self.id, e);
                        }
                    }
                } else {
                    // UserManager 不可用，降级为简单检查
                    if !user_id.is_empty() && !token.is_empty() {
                        self.state = SessionState::Authenticated {
                            user_id: user_id.clone(),
                        };

                        let response = ServerMessage::AuthResponse {
                            success: true,
                            user_id: user_id.clone(),
                            message: "Authentication successful (fallback mode)".to_string(),
                        };

                        if let Ok(json) = serde_json::to_string(&response) {
                            ctx.text(json);
                        }

                        log::warn!(
                            "Session {} authenticated in fallback mode (UserManager not available)",
                            self.id
                        );
                    } else {
                        let response = ServerMessage::AuthResponse {
                            success: false,
                            user_id: String::new(),
                            message: "Invalid credentials".to_string(),
                        };

                        if let Ok(json) = serde_json::to_string(&response) {
                            ctx.text(json);
                        }
                    }
                }
            }

            ClientMessage::Subscribe {
                channels,
                instruments,
            } => {
                // 更新订阅列表
                for channel in channels {
                    if !self.subscribed_channels.contains(channel) {
                        self.subscribed_channels.push(channel.clone());
                    }
                }

                for instrument in instruments {
                    if !self.subscribed_instruments.contains(instrument) {
                        self.subscribed_instruments.push(instrument.clone());
                    }
                }

                // 订阅市场数据
                if let Some(ref broadcaster) = self.market_broadcaster {
                    let receiver = broadcaster.subscribe(
                        self.id.clone(),
                        instruments.clone(),
                        channels.clone(),
                    );
                    self.market_data_receiver = Some(receiver);

                    // 启动市场数据监听
                    self.start_market_data_listener(ctx);
                }

                let response = ServerMessage::SubscribeResponse {
                    success: true,
                    channels: channels.clone(),
                    instruments: instruments.clone(),
                    message: "Subscribed successfully".to_string(),
                };

                if let Ok(json) = serde_json::to_string(&response) {
                    ctx.text(json);
                }

                log::info!(
                    "Session {} subscribed to channels: {:?}, instruments: {:?}",
                    self.id,
                    channels,
                    instruments
                );
            }

            ClientMessage::Unsubscribe {
                channels,
                instruments,
            } => {
                // 从订阅列表中移除
                self.subscribed_channels.retain(|ch| !channels.contains(ch));
                self.subscribed_instruments
                    .retain(|inst| !instruments.contains(inst));

                // 如果所有订阅都取消了，注销订阅
                if self.subscribed_channels.is_empty() && self.subscribed_instruments.is_empty() {
                    if let Some(ref broadcaster) = self.market_broadcaster {
                        broadcaster.unsubscribe(&self.id);
                    }
                    self.market_data_receiver = None;
                }

                let response = ServerMessage::SubscribeResponse {
                    success: true,
                    channels: channels.clone(),
                    instruments: instruments.clone(),
                    message: "Unsubscribed successfully".to_string(),
                };

                if let Ok(json) = serde_json::to_string(&response) {
                    ctx.text(json);
                }

                log::info!(
                    "Session {} unsubscribed from channels: {:?}, instruments: {:?}",
                    self.id,
                    channels,
                    instruments
                );
            }

            ClientMessage::Ping => {
                let response = ServerMessage::Pong;
                if let Ok(json) = serde_json::to_string(&response) {
                    ctx.text(json);
                }
            }

            _ => {
                // 其他消息需要认证
                if let SessionState::Authenticated { user_id } = &self.state {
                    // 发送消息到业务逻辑处理器
                    let session_msg = WsSessionMessage {
                        session_id: self.id.clone(),
                        user_id: Some(user_id.clone()),
                        message: msg,
                    };

                    if let Err(e) = self.message_sender.send(session_msg) {
                        log::error!("Failed to send message to handler: {}", e);
                    }
                } else {
                    let error = ServerMessage::Error {
                        code: 401,
                        message: "Not authenticated".to_string(),
                    };

                    if let Ok(json) = serde_json::to_string(&error) {
                        ctx.text(json);
                    }
                }
            }
        }
    }

    /// 发送服务端消息
    pub fn send_message(&self, msg: ServerMessage, ctx: &mut ws::WebsocketContext<Self>) {
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("WebSocket session {} started", self.id);

        // 注册到会话映射
        if let Some(ref sessions) = self.sessions {
            sessions.write().insert(self.id.clone(), ctx.address());
        }

        self.start_heartbeat(ctx);

        // 启动通知监听器（如果有的话）
        if let Some(receiver) = &self.notification_receiver {
            let receiver = receiver.clone();
            ctx.run_interval(Duration::from_millis(10), move |act, ctx| {
                while let Ok(notification) = receiver.try_recv() {
                    // 转换通知为服务端消息
                    match notification {
                        crate::exchange::Notification::Trade(trade) => {
                            let msg = ServerMessage::Trade {
                                trade_id: trade.trade_id,
                                order_id: trade.order_id,
                                instrument_id: trade.instrument_id,
                                direction: trade.direction,
                                offset: trade.offset,
                                price: trade.price,
                                volume: trade.volume,
                                timestamp: trade.timestamp,
                            };
                            act.send_message(msg, ctx);
                        }
                        crate::exchange::Notification::OrderStatus(status) => {
                            let msg = ServerMessage::OrderStatus {
                                order_id: status.order_id,
                                exchange_id: status.exchange_id,
                                instrument_id: status.instrument_id,
                                exchange_order_id: status.exchange_order_id,
                                direction: status.direction,
                                offset: status.offset,
                                price_type: status.price_type,
                                volume: status.volume,
                                price: status.price,
                                status: status.status,
                                timestamp: status.timestamp,
                            };
                            act.send_message(msg, ctx);
                        }
                        crate::exchange::Notification::AccountUpdate(account) => {
                            let frozen = account.balance - account.available;
                            let msg = ServerMessage::AccountUpdate {
                                balance: account.balance,
                                available: account.available,
                                frozen,
                                margin: account.margin,
                                profit: account.position_profit,
                                risk_ratio: account.risk_ratio,
                                timestamp: account.timestamp,
                            };
                            act.send_message(msg, ctx);
                        }
                    }
                }
            });
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("WebSocket session {} stopped", self.id);

        // 从会话映射注销
        if let Some(ref sessions) = self.sessions {
            sessions.write().remove(&self.id);
        }
    }
}

/// 处理文本消息
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.heartbeat = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.heartbeat = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                self.heartbeat = Instant::now();

                // 解析客户端消息
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(client_msg) => {
                        self.handle_client_message(client_msg, ctx);
                    }
                    Err(e) => {
                        log::error!("Failed to parse client message: {}", e);
                        let error = ServerMessage::Error {
                            code: 400,
                            message: format!("Invalid message format: {}", e),
                        };
                        if let Ok(json) = serde_json::to_string(&error) {
                            ctx.text(json);
                        }
                    }
                }
            }
            Ok(ws::Message::Binary(_)) => {
                log::warn!("Binary messages not supported");
            }
            Ok(ws::Message::Close(reason)) => {
                log::info!("WebSocket session {} closed: {:?}", self.id, reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

/// 处理外部发送的消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendMessage(pub ServerMessage);

impl Handler<SendMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: SendMessage, ctx: &mut Self::Context) {
        self.send_message(msg.0, ctx);
    }
}
