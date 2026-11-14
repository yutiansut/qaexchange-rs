//! WebSocket 消息处理器

use actix::Addr;
use crate::core::account_ext::AccountExt;   // safe_risk_ratio @yutiansut @quantaxis
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
                            // ⚠️ `acc.accounts.<派生字段>` 盘中**永远是开户时的初始值** ——
                    // qars 只在 `settle()` 里整体重建 self.accounts(account.rs:574),
                    // 成交/撤单都不回写。实测 NOISE_SELL 成交 6.8 万笔后
                    // accounts.balance 仍是 1,500,000,000(init_cash)、risk_ratio 仍是 0.0。
                    // 本处已持写锁(为 get_margin 取的),改用动态 getter 零额外成本。
                    // @yutiansut @quantaxis
                    // ⚠️ 用 `qifi_balance()` 而非 qars 的 `get_balance()` @yutiansut @quantaxis
                    //
                    // qars 的 `get_balance()` 依赖它内部的 `static_balance`(语义 = 初始资金),
                    // 一旦该字段被写成 QIFI 语义(含 deposit)就会把入金算两遍。
                    // HTTP 侧的投影层已按规范重算,若这里仍用 qars 原生值,
                    // **同一个账户在 HTTP 与 WebSocket 上会显示相差一份 deposit 的两个余额**,
                    // 且数字随哪个通道最后更新而跳变。统一走 AccountExt::qifi_balance。
                    let qifi_bal = acc.qifi_balance();
                    let frozen = qifi_bal - acc.money;
                    // ✨ 保证金 = 持仓保证金 + 冻结保证金（待成交订单）@yutiansut @quantaxis
                    let position_margin = acc.get_margin();
                    let frozen_margin = acc.get_frozen_margin();
                    let margin = position_margin + frozen_margin;
                    let data = serde_json::json!({
                        "account": {
                            "user_id": acc.account_cookie,
                            "balance": qifi_bal,
                            "available": acc.money,
                            "frozen": frozen,
                            "margin": margin,  // ✨ 使用动态计算的值
                            "profit": acc.get_closeprofit(),
                            // 与上面的 qifi_bal 保持同一分母,否则 risk_ratio != margin/balance
                            "risk_ratio": if qifi_bal > 0.0 {
                                let r = margin / qifi_bal;
                                if r.is_finite() { r } else { 0.0 }
                            } else { 0.0 },
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
                        // @yutiansut @quantaxis: 直接用 qars 的 get_qifi_slice() 获取完整 QIFI
                        let mut acc = account.write();
                        let qifi = acc.get_qifi_slice();
                        // positions 已经是 HashMap<String, Position>，直接序列化
                        let positions: Vec<_> = if let Some(ref inst_id) = instrument_id {
                            qifi.positions.get(inst_id)
                                .map(|pos| vec![serde_json::to_value(pos).unwrap()])
                                .unwrap_or_default()
                        } else {
                            qifi.positions.values()
                                .map(|pos| serde_json::to_value(pos).unwrap())
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
