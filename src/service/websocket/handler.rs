//! WebSocket 消息处理器

use actix::Addr;
use crossbeam::channel::{unbounded, Receiver, Sender};
use log;
use std::collections::HashMap;
use std::sync::Arc;

use super::messages::{ClientMessage, ServerMessage};
use super::session::{SendMessage, WsSession, WsSessionMessage};
use crate::exchange::order_router::{CancelOrderRequest, SubmitOrderRequest};
use crate::exchange::{AccountManager, OrderRouter};

/// WebSocket 消息处理器
pub struct WsMessageHandler {
    /// 订单路由器
    order_router: Arc<OrderRouter>,

    /// 账户管理器
    account_mgr: Arc<AccountManager>,

    /// 消息接收器
    message_receiver: Receiver<WsSessionMessage>,

    /// 会话地址映射 (session_id -> Addr<WsSession>)
    sessions: Arc<parking_lot::RwLock<HashMap<String, Addr<WsSession>>>>,
}

impl WsMessageHandler {
    pub fn new(
        order_router: Arc<OrderRouter>,
        account_mgr: Arc<AccountManager>,
        message_receiver: Receiver<WsSessionMessage>,
    ) -> Self {
        Self {
            order_router,
            account_mgr,
            message_receiver,
            sessions: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// 注册会话
    pub fn register_session(&self, session_id: String, addr: Addr<WsSession>) {
        self.sessions.write().insert(session_id.clone(), addr);
        log::info!("Registered WebSocket session: {}", session_id);
    }

    /// 注销会话
    pub fn unregister_session(&self, session_id: &str) {
        self.sessions.write().remove(session_id);
        log::info!("Unregistered WebSocket session: {}", session_id);
    }

    /// 获取会话地址
    pub fn get_session(&self, session_id: &str) -> Option<Addr<WsSession>> {
        self.sessions.read().get(session_id).cloned()
    }

    /// 获取消息接收器的引用（用于外部启动处理循环）
    pub fn get_receiver(&self) -> &Receiver<WsSessionMessage> {
        &self.message_receiver
    }

    /// 启动处理循环（消费 self）
    pub fn start(self) {
        std::thread::spawn(move || {
            log::info!("WebSocket message handler started");

            loop {
                match self.message_receiver.recv() {
                    Ok(msg) => {
                        if let Err(e) = self.handle_message(msg) {
                            log::error!("Error handling message: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Message receiver error: {}", e);
                        break;
                    }
                }
            }

            log::info!("WebSocket message handler stopped");
        });
    }

    /// 处理消息
    fn handle_message(&self, msg: WsSessionMessage) -> Result<(), String> {
        let session_addr = self
            .get_session(&msg.session_id)
            .ok_or_else(|| format!("Session not found: {}", msg.session_id))?;

        let user_id = msg
            .user_id
            .as_ref()
            .ok_or_else(|| "User ID not found".to_string())?;

        match msg.message {
            ClientMessage::SubmitOrder {
                account_id: client_account_id, // ✨ 新增字段（来自客户端）
                instrument_id,
                direction,
                offset,
                volume,
                price,
                order_type,
            } => {
                // 服务层：验证账户所有权并获取 account_id
                let account_id = if let Some(ref acc_id) = client_account_id {
                    // ✅ 客户端明确传递了 account_id，验证所有权
                    if let Err(e) = self.account_mgr.verify_account_ownership(acc_id, user_id) {
                        let server_msg = ServerMessage::OrderResponse {
                            success: false,
                            order_id: None,
                            error_code: Some(4003),
                            error_message: Some(format!("Account verification failed: {}", e)),
                        };
                        session_addr.do_send(SendMessage(server_msg));
                        return Ok(());
                    }
                    acc_id.clone()
                } else {
                    // ⚠️ 向后兼容：客户端未传递 account_id，使用默认账户
                    log::warn!("DEPRECATED: WebSocket client did not provide account_id for user {}. This behavior will be removed in future versions.", user_id);

                    match self.account_mgr.get_default_account(user_id) {
                        Ok(account_arc) => {
                            let acc = account_arc.read();
                            acc.account_cookie.clone()
                        }
                        Err(e) => {
                            let server_msg = ServerMessage::OrderResponse {
                                success: false,
                                order_id: None,
                                error_code: Some(4000),
                                error_message: Some(format!(
                                    "Account not found for user {}: {}",
                                    user_id, e
                                )),
                            };
                            session_addr.do_send(SendMessage(server_msg));
                            return Ok(());
                        }
                    }
                };

                let req = SubmitOrderRequest {
                    account_id, // 交易层只关心 account_id
                    instrument_id,
                    direction,
                    offset,
                    volume,
                    price,
                    order_type,
                    time_condition: None,
                    volume_condition: None,
                };

                let response = self.order_router.submit_order(req);

                let server_msg = ServerMessage::OrderResponse {
                    success: response.success,
                    order_id: response.order_id,
                    error_code: response.error_code,
                    error_message: response.error_message,
                };

                session_addr.do_send(SendMessage(server_msg));
            }

            ClientMessage::CancelOrder {
                account_id: client_account_id,
                order_id,
            } => {
                // 服务层：验证账户所有权并获取 account_id
                let account_id = if let Some(ref acc_id) = client_account_id {
                    // ✅ 客户端明确传递了 account_id，验证所有权
                    if let Err(e) = self.account_mgr.verify_account_ownership(acc_id, user_id) {
                        let server_msg = ServerMessage::OrderResponse {
                            success: false,
                            order_id: Some(order_id.clone()),
                            error_code: Some(4003),
                            error_message: Some(format!("Account verification failed: {}", e)),
                        };
                        session_addr.do_send(SendMessage(server_msg));
                        return Ok(());
                    }
                    acc_id.clone()
                } else {
                    // ⚠️ 向后兼容：客户端未传递 account_id，使用默认账户
                    log::warn!("DEPRECATED: WebSocket client did not provide account_id for user {}. This behavior will be removed in future versions.", user_id);

                    match self.account_mgr.get_default_account(user_id) {
                        Ok(account_arc) => {
                            let acc = account_arc.read();
                            acc.account_cookie.clone()
                        }
                        Err(e) => {
                            let server_msg = ServerMessage::OrderResponse {
                                success: false,
                                order_id: Some(order_id.clone()),
                                error_code: Some(4000),
                                error_message: Some(format!(
                                    "Account not found for user {}: {}",
                                    user_id, e
                                )),
                            };
                            session_addr.do_send(SendMessage(server_msg));
                            return Ok(());
                        }
                    }
                };

                let req = CancelOrderRequest {
                    account_id, // 交易层只关心 account_id
                    order_id: order_id.clone(),
                };

                match self.order_router.cancel_order(req) {
                    Ok(_) => {
                        let server_msg = ServerMessage::OrderResponse {
                            success: true,
                            order_id: Some(order_id),
                            error_code: None,
                            error_message: None,
                        };
                        session_addr.do_send(SendMessage(server_msg));
                    }
                    Err(e) => {
                        let server_msg = ServerMessage::OrderResponse {
                            success: false,
                            order_id: Some(order_id),
                            error_code: Some(1001),
                            error_message: Some(format!("{:?}", e)),
                        };
                        session_addr.do_send(SendMessage(server_msg));
                    }
                }
            }

            ClientMessage::QueryOrder { order_id } => {
                match self.order_router.query_order(&order_id) {
                    Some(order) => {
                        let data = serde_json::json!({
                            "order": {
                                "user_id": order.user_id,
                                "instrument_id": order.instrument_id,
                                "direction": order.direction,
                                "offset": order.offset,
                                "volume": order.volume_orign,
                                "price": order.limit_price,
                                "status": order.status,
                            }
                        });

                        let server_msg = ServerMessage::QueryResponse {
                            request_type: "query_order".to_string(),
                            data,
                        };
                        session_addr.do_send(SendMessage(server_msg));
                    }
                    None => {
                        let server_msg = ServerMessage::Error {
                            code: 1002,
                            message: format!("Order not found: {}", order_id),
                        };
                        session_addr.do_send(SendMessage(server_msg));
                    }
                }
            }

            ClientMessage::QueryAccount => match self.account_mgr.get_account(user_id) {
                Ok(account) => {
                    // ✨ 使用 write() 以便调用 get_margin() 动态计算 @yutiansut @quantaxis
                    let mut acc = account.write();
                    let frozen = acc.accounts.balance - acc.money;
                    let margin = acc.get_margin();  // ✨ 修复: 使用动态计算的 margin
                    let data = serde_json::json!({
                        "account": {
                            "user_id": acc.account_cookie,
                            "balance": acc.accounts.balance,
                            "available": acc.money,
                            "frozen": frozen,
                            "margin": margin,  // ✨ 使用动态计算的值
                            "profit": acc.accounts.close_profit,
                            "risk_ratio": acc.accounts.risk_ratio,
                        }
                    });

                    let server_msg = ServerMessage::QueryResponse {
                        request_type: "query_account".to_string(),
                        data,
                    };
                    session_addr.do_send(SendMessage(server_msg));
                }
                Err(e) => {
                    let server_msg = ServerMessage::Error {
                        code: 1003,
                        message: format!("Query account failed: {:?}", e),
                    };
                    session_addr.do_send(SendMessage(server_msg));
                }
            },

            ClientMessage::QueryPosition { instrument_id } => {
                match self.account_mgr.get_account(user_id) {
                    Ok(account) => {
                        let acc = account.read();
                        let positions: Vec<_> = if let Some(ref inst_id) = instrument_id {
                            // 查询特定合约的持仓
                            acc.hold.iter()
                                .filter(|(code, _)| *code == inst_id)
                                .map(|(code, pos)| {
                                    serde_json::json!({
                                        "instrument_id": code,
                                        "volume_long": pos.volume_long_today + pos.volume_long_his,
                                        "volume_short": pos.volume_short_today + pos.volume_short_his,
                                        "cost_long": pos.open_price_long,
                                        "cost_short": pos.open_price_short,
                                    })
                                })
                                .collect()
                        } else {
                            // 查询所有持仓
                            acc.hold.iter()
                                .map(|(code, pos)| {
                                    serde_json::json!({
                                        "instrument_id": code,
                                        "volume_long": pos.volume_long_today + pos.volume_long_his,
                                        "volume_short": pos.volume_short_today + pos.volume_short_his,
                                        "cost_long": pos.open_price_long,
                                        "cost_short": pos.open_price_short,
                                    })
                                })
                                .collect()
                        };

                        let data = serde_json::json!({
                            "positions": positions
                        });

                        let server_msg = ServerMessage::QueryResponse {
                            request_type: "query_position".to_string(),
                            data,
                        };
                        session_addr.do_send(SendMessage(server_msg));
                    }
                    Err(e) => {
                        let server_msg = ServerMessage::Error {
                            code: 1004,
                            message: format!("Query position failed: {:?}", e),
                        };
                        session_addr.do_send(SendMessage(server_msg));
                    }
                }
            }

            _ => {
                log::warn!("Unhandled message type: {:?}", msg.message);
            }
        }

        Ok(())
    }
}

/// 创建消息处理器和发送器
pub fn create_handler(
    order_router: Arc<OrderRouter>,
    account_mgr: Arc<AccountManager>,
) -> (
    WsMessageHandler,
    Sender<WsSessionMessage>,
    Arc<parking_lot::RwLock<HashMap<String, Addr<WsSession>>>>,
) {
    let (sender, receiver) = unbounded();
    let sessions = Arc::new(parking_lot::RwLock::new(HashMap::new()));
    let handler = WsMessageHandler {
        order_router,
        account_mgr,
        message_receiver: receiver,
        sessions: sessions.clone(),
    };
    (handler, sender, sessions)
}
