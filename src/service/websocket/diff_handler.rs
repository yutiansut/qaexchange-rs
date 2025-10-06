//! DIFF 协议 WebSocket 处理器
//!
//! 实现 DIFF 协议的 WebSocket 消息处理逻辑：
//! - peek_message 阻塞等待机制
//! - rtn_data 差分推送
//! - 零拷贝优化
//!
//! # 性能优化
//!
//! - **零拷贝**: 使用 Arc 共享 SnapshotManager
//! - **低延迟**: peek() 使用 Tokio Notify，零轮询开销
//! - **高并发**: DashMap 支持万级用户并发
//!
//! # 使用示例
//!
//! ```rust
//! use qaexchange::service::websocket::diff_handler::DiffHandler;
//! use qaexchange::protocol::diff::snapshot::SnapshotManager;
//! use qaexchange::exchange::AccountManager;
//! use std::sync::Arc;
//!
//! let snapshot_mgr = Arc::new(SnapshotManager::new());
//! let account_mgr = Arc::new(AccountManager::new());
//! let handler = DiffHandler::new(snapshot_mgr, account_mgr);
//! ```

use actix::{Actor, ActorContext, AsyncContext, StreamHandler, Handler as ActixHandler, Message as ActixMessage, Addr, Context, WrapFuture};
use actix_web_actors::ws;
use std::sync::Arc;
use std::time::Duration;
use log;

use super::diff_messages::{DiffClientMessage, DiffServerMessage};
use crate::protocol::diff::snapshot::SnapshotManager;
use crate::user::UserManager;
use crate::exchange::{OrderRouter, AccountManager};
use crate::market::MarketDataBroadcaster;

/// DIFF 协议消息处理器
pub struct DiffHandler {
    /// 业务快照管理器（共享引用）
    pub(crate) snapshot_mgr: Arc<SnapshotManager>,

    /// 用户管理器
    pub(crate) user_manager: Option<Arc<UserManager>>,

    /// 账户管理器（用于账户所有权验证）✨ Phase 10
    pub(crate) account_mgr: Arc<AccountManager>,

    /// 订单路由器
    pub(crate) order_router: Option<Arc<OrderRouter>>,

    /// 市场数据广播器
    pub(crate) market_broadcaster: Option<Arc<MarketDataBroadcaster>>,
}

impl DiffHandler {
    /// 创建新的 DIFF 处理器
    pub fn new(snapshot_mgr: Arc<SnapshotManager>, account_mgr: Arc<AccountManager>) -> Self {
        Self {
            snapshot_mgr,
            user_manager: None,
            account_mgr,
            order_router: None,
            market_broadcaster: None,
        }
    }

    /// 设置用户管理器
    pub fn with_user_manager(mut self, user_manager: Arc<UserManager>) -> Self {
        self.user_manager = Some(user_manager);
        self
    }

    /// 设置订单路由器
    pub fn with_order_router(mut self, order_router: Arc<OrderRouter>) -> Self {
        self.order_router = Some(order_router);
        self
    }

    /// 设置市场数据广播器
    pub fn with_market_broadcaster(mut self, market_broadcaster: Arc<MarketDataBroadcaster>) -> Self {
        self.market_broadcaster = Some(market_broadcaster);
        self
    }

    /// 处理 DIFF 客户端消息
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户ID
    /// * `msg` - DIFF 客户端消息
    /// * `ctx` - WebSocket 上下文
    pub async fn handle_diff_message(
        &self,
        user_id: &str,
        msg: DiffClientMessage,
        ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        match msg {
            DiffClientMessage::PeekMessage => {
                self.handle_peek_message(user_id, ctx_addr).await;
            }

            DiffClientMessage::ReqLogin { bid, user_name, password } => {
                log::info!("DIFF login request: user_name={}, bid={:?}", user_name, bid);
                self.handle_login(user_name, password, ctx_addr).await;
            }

            DiffClientMessage::SubscribeQuote { ins_list } => {
                log::info!("DIFF subscribe quote: ins_list={}", ins_list);
                self.handle_subscribe_quote(user_id, ins_list, ctx_addr).await;
            }

            DiffClientMessage::InsertOrder {
                user_id: order_user_id,
                account_id,  // ✨ 新增字段
                order_id,
                exchange_id,
                instrument_id,
                direction,
                offset,
                volume,
                price_type,
                limit_price,
                volume_condition,
                time_condition,
            } => {
                log::info!("DIFF insert order: user_id={}, account_id={:?}, order_id={}", order_user_id, account_id, order_id);
                self.handle_insert_order(
                    user_id,
                    order_user_id,
                    account_id,  // ✨ 传递 account_id
                    order_id,
                    exchange_id,
                    instrument_id,
                    direction,
                    offset,
                    volume,
                    price_type,
                    limit_price,
                    ctx_addr,
                ).await;
            }

            DiffClientMessage::CancelOrder { user_id: cancel_user_id, account_id, order_id } => {
                log::info!("DIFF cancel order: user_id={}, account_id={:?}, order_id={}", cancel_user_id, account_id, order_id);
                self.handle_cancel_order(user_id, cancel_user_id, account_id, order_id, ctx_addr).await;  // ✨ 传递 account_id
            }

            DiffClientMessage::SetChart { chart_id, ins_list, duration, view_width } => {
                log::info!(
                    "DIFF set chart: chart_id={}, ins_list={}, duration={}, view_width={}",
                    chart_id, ins_list, duration, view_width
                );
                self.handle_set_chart(user_id, chart_id, ins_list, duration, view_width, ctx_addr).await;
            }
        }
    }

    /// 处理登录请求
    async fn handle_login(
        &self,
        username: String,
        password: String,
        ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        if let Some(ref user_mgr) = self.user_manager {
            // 使用 UserManager 进行登录验证
            let login_req = crate::user::UserLoginRequest {
                username: username.clone(),
                password,
            };

            match user_mgr.login(login_req) {
                Ok(login_resp) => {
                    if login_resp.success {
                        // 登录成功，初始化用户快照
                        let user_id = login_resp.user_id.clone().unwrap_or_default();
                        self.snapshot_mgr.initialize_user(&user_id).await;

                        // 发送登录成功通知
                        let notify_patch = serde_json::json!({
                            "notify": {
                                "login_success": {
                                    "type": "MESSAGE",
                                    "level": "INFO",
                                    "code": 0,
                                    "content": format!("Login successful for user: {}", username)
                                }
                            },
                            "user_id": user_id,
                            "username": username
                        });

                        let rtn_data = DiffServerMessage::RtnData {
                            data: vec![notify_patch],
                        };

                        ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                        log::info!("DIFF login successful: user={}, user_id={}", username, user_id);
                    } else {
                        // 登录失败
                        let notify_patch = serde_json::json!({
                            "notify": {
                                "login_failed": {
                                    "type": "MESSAGE",
                                    "level": "ERROR",
                                    "code": 1001,
                                    "content": login_resp.message
                                }
                            }
                        });

                        let rtn_data = DiffServerMessage::RtnData {
                            data: vec![notify_patch],
                        };

                        ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                        log::warn!("DIFF login failed for user: {}", username);
                    }
                }
                Err(e) => {
                    // 登录错误
                    let notify_patch = serde_json::json!({
                        "notify": {
                            "login_error": {
                                "type": "MESSAGE",
                                "level": "ERROR",
                                "code": 1002,
                                "content": format!("Login error: {}", e)
                            }
                        }
                    });

                    let rtn_data = DiffServerMessage::RtnData {
                        data: vec![notify_patch],
                    };

                    ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                    log::error!("DIFF login error for user {}: {}", username, e);
                }
            }
        } else {
            // UserManager 不可用
            let notify_patch = serde_json::json!({
                "notify": {
                    "service_error": {
                        "type": "MESSAGE",
                        "level": "ERROR",
                        "code": 1003,
                        "content": "User management service is not available"
                    }
                }
            });

            let rtn_data = DiffServerMessage::RtnData {
                data: vec![notify_patch],
            };

            ctx_addr.do_send(SendDiffMessage { message: rtn_data });
            log::error!("DIFF login failed: UserManager not available");
        }
    }

    /// 处理行情订阅请求
    async fn handle_subscribe_quote(
        &self,
        user_id: &str,
        ins_list: String,
        ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        // 解析合约列表（逗号分隔）
        let instruments: Vec<String> = ins_list
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if instruments.is_empty() {
            // 空列表表示取消订阅
            let notify_patch = serde_json::json!({
                "notify": {
                    "unsubscribe": {
                        "type": "MESSAGE",
                        "level": "INFO",
                        "code": 0,
                        "content": "Unsubscribed from all quotes"
                    }
                },
                "ins_list": ""
            });

            let rtn_data = DiffServerMessage::RtnData {
                data: vec![notify_patch],
            };

            ctx_addr.do_send(SendDiffMessage { message: rtn_data });
            log::info!("User {} unsubscribed from all quotes", user_id);
            return;
        }

        // 更新用户快照中的ins_list
        // TODO: 实际应该调用 SnapshotManager::update() 更新用户快照
        // self.snapshot_mgr.update_user(user_id, serde_json::json!({
        //     "ins_list": ins_list
        // })).await;

        // 发送订阅确认和初始行情数据
        let notify_patch = serde_json::json!({
            "notify": {
                "subscribe_success": {
                    "type": "MESSAGE",
                    "level": "INFO",
                    "code": 0,
                    "content": format!("Subscribed to {} instruments", instruments.len())
                }
            },
            "ins_list": ins_list,
            "quotes": {
                // 这里应该包含初始行情快照
                // 实际实现中应该从MarketDataBroadcaster获取当前快照
            }
        });

        let rtn_data = DiffServerMessage::RtnData {
            data: vec![notify_patch],
        };

        ctx_addr.do_send(SendDiffMessage { message: rtn_data });

        log::info!(
            "User {} subscribed to quotes: {:?}",
            user_id,
            instruments
        );

        // TODO: 实际应该从 MarketDataBroadcaster 订阅并持续推送行情更新
        // 目前仅完成订阅确认，行情推送需要与 MarketDataBroadcaster 集成
    }

    /// 处理下单请求
    async fn handle_insert_order(
        &self,
        session_user_id: &str,
        order_user_id: String,
        client_account_id: Option<String>,  // ✨ 新增参数
        order_id: String,
        exchange_id: String,
        instrument_id: String,
        direction: String,
        offset: String,
        volume: i64,
        price_type: String,
        limit_price: Option<f64>,
        ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        // 验证用户权限（session用户必须与订单用户匹配）
        if session_user_id != order_user_id {
            let notify_patch = serde_json::json!({
                "notify": {
                    "order_error": {
                        "type": "MESSAGE",
                        "level": "ERROR",
                        "code": 2001,
                        "content": "User ID mismatch"
                    }
                }
            });

            let rtn_data = DiffServerMessage::RtnData {
                data: vec![notify_patch],
            };

            ctx_addr.do_send(SendDiffMessage { message: rtn_data });
            log::warn!("DIFF insert order failed: user mismatch (session={}, order={})", session_user_id, order_user_id);
            return;
        }

        if let Some(ref order_router) = self.order_router {
            // 转换价格类型
            let order_type = match price_type.as_str() {
                "LIMIT" => "LIMIT",
                "MARKET" | "ANY" => "MARKET",
                _ => "LIMIT",
            };

            // 服务层：验证账户所有权并获取 account_id
            let account_id = if let Some(ref acc_id) = client_account_id {
                // ✅ 客户端明确传递了 account_id，验证所有权
                if let Err(e) = self.account_mgr.verify_account_ownership(acc_id, &order_user_id) {
                    let notify_patch = serde_json::json!({
                        "notify": {
                            "order_error": {
                                "type": "MESSAGE",
                                "level": "ERROR",
                                "code": 4003,
                                "content": format!("Account verification failed: {}", e)
                            }
                        }
                    });

                    let rtn_data = DiffServerMessage::RtnData {
                        data: vec![notify_patch],
                    };

                    ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                    log::warn!("DIFF insert order failed: account verification failed for user {}: {}", order_user_id, e);
                    return;
                }
                acc_id.clone()
            } else {
                // ⚠️ 向后兼容：客户端未传递 account_id，使用用户的第一个账户
                log::warn!("DEPRECATED: DIFF client did not provide account_id for user {}. This behavior will be removed in future versions.", order_user_id);

                if let Some(ref user_mgr) = self.user_manager {
                    match user_mgr.get_user_accounts(&order_user_id) {
                        Ok(accounts) if !accounts.is_empty() => accounts[0].clone(),
                        Ok(_) => {
                            log::warn!("DIFF insert order failed: no accounts found for user {}", order_user_id);
                            return;
                        },
                        Err(e) => {
                            log::warn!("DIFF insert order failed: user lookup error: {}", e);
                            return;
                        }
                    }
                } else {
                    log::warn!("DIFF insert order failed: user_manager not available");
                    return;
                }
            };

            // 构造OrderRouter请求（交易层只关心 account_id）
            let req = crate::exchange::order_router::SubmitOrderRequest {
                account_id,
                instrument_id: instrument_id.clone(),
                direction: direction.clone(),
                offset: offset.clone(),
                volume: volume as f64,
                price: limit_price.unwrap_or(0.0),
                order_type: order_type.to_string(),
            };

            // 提交订单
            let response = order_router.submit_order(req);

            if response.success {
                // 下单成功，发送确认通知
                let notify_patch = serde_json::json!({
                    "trade": {
                        order_user_id.clone(): {
                            "orders": {
                                order_id.clone(): {
                                    "order_id": order_id.clone(),
                                    "user_id": order_user_id.clone(),
                                    "exchange_id": exchange_id.clone(),
                                    "instrument_id": instrument_id.clone(),
                                    "direction": direction.clone(),
                                    "offset": offset.clone(),
                                    "volume_orign": volume,
                                    "price_type": price_type.clone(),
                                    "limit_price": limit_price.unwrap_or(0.0),
                                    "status": response.status.unwrap_or("SUBMITTED".to_string())
                                }
                            }
                        }
                    }
                });

                let rtn_data = DiffServerMessage::RtnData {
                    data: vec![notify_patch],
                };

                ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                log::info!("DIFF insert order success: order_id={}", order_id);
            } else {
                // 下单失败，发送错误通知
                let notify_patch = serde_json::json!({
                    "notify": {
                        "order_rejected": {
                            "type": "MESSAGE",
                            "level": "ERROR",
                            "code": response.error_code.unwrap_or(2002),
                            "content": response.error_message.unwrap_or("Order rejected".to_string())
                        }
                    }
                });

                let rtn_data = DiffServerMessage::RtnData {
                    data: vec![notify_patch],
                };

                ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                log::warn!("DIFF insert order rejected: order_id={}", order_id);
            }
        } else {
            // OrderRouter 不可用
            let notify_patch = serde_json::json!({
                "notify": {
                    "service_error": {
                        "type": "MESSAGE",
                        "level": "ERROR",
                        "code": 2003,
                        "content": "Order routing service is not available"
                    }
                }
            });

            let rtn_data = DiffServerMessage::RtnData {
                data: vec![notify_patch],
            };

            ctx_addr.do_send(SendDiffMessage { message: rtn_data });
            log::error!("DIFF insert order failed: OrderRouter not available");
        }
    }

    /// 处理撤单请求
    async fn handle_cancel_order(
        &self,
        session_user_id: &str,
        cancel_user_id: String,
        client_account_id: Option<String>,  // ✨ 新增参数
        order_id: String,
        ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        // 验证用户权限
        if session_user_id != cancel_user_id {
            let notify_patch = serde_json::json!({
                "notify": {
                    "cancel_error": {
                        "type": "MESSAGE",
                        "level": "ERROR",
                        "code": 3001,
                        "content": "User ID mismatch"
                    }
                }
            });

            let rtn_data = DiffServerMessage::RtnData {
                data: vec![notify_patch],
            };

            ctx_addr.do_send(SendDiffMessage { message: rtn_data });
            log::warn!("DIFF cancel order failed: user mismatch");
            return;
        }

        if let Some(ref order_router) = self.order_router {
            // 服务层：验证账户所有权并获取 account_id
            let account_id = if let Some(ref acc_id) = client_account_id {
                // ✅ 客户端明确传递了 account_id，验证所有权
                if let Err(e) = self.account_mgr.verify_account_ownership(acc_id, &cancel_user_id) {
                    let notify_patch = serde_json::json!({
                        "notify": {
                            "cancel_error": {
                                "type": "MESSAGE",
                                "level": "ERROR",
                                "code": 4003,
                                "content": format!("Account verification failed: {}", e)
                            }
                        }
                    });

                    let rtn_data = DiffServerMessage::RtnData {
                        data: vec![notify_patch],
                    };

                    ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                    log::warn!("DIFF cancel order failed: account verification failed for user {}: {}", cancel_user_id, e);
                    return;
                }
                acc_id.clone()
            } else {
                // ⚠️ 向后兼容：客户端未传递 account_id，使用用户的第一个账户
                log::warn!("DEPRECATED: DIFF client did not provide account_id for user {}. This behavior will be removed in future versions.", cancel_user_id);

                if let Some(ref user_mgr) = self.user_manager {
                    match user_mgr.get_user_accounts(&cancel_user_id) {
                        Ok(accounts) if !accounts.is_empty() => accounts[0].clone(),
                        Ok(_) => {
                            log::warn!("DIFF cancel order failed: no accounts found for user {}", cancel_user_id);
                            return;
                        },
                        Err(e) => {
                            log::warn!("DIFF cancel order failed: user lookup error: {}", e);
                            return;
                        }
                    }
                } else {
                    log::warn!("DIFF cancel order failed: user_manager not available");
                    return;
                }
            };

            let req = crate::exchange::order_router::CancelOrderRequest {
                account_id,  // 交易层只关心 account_id
                order_id: order_id.clone(),
            };

            match order_router.cancel_order(req) {
                Ok(_) => {
                    // 撤单成功
                    let notify_patch = serde_json::json!({
                        "trade": {
                            cancel_user_id: {
                                "orders": {
                                    order_id.clone(): {
                                        "status": "CANCELLED"
                                    }
                                }
                            }
                        }
                    });

                    let rtn_data = DiffServerMessage::RtnData {
                        data: vec![notify_patch],
                    };

                    ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                    log::info!("DIFF cancel order success: order_id={}", order_id);
                }
                Err(e) => {
                    // 撤单失败
                    let notify_patch = serde_json::json!({
                        "notify": {
                            "cancel_failed": {
                                "type": "MESSAGE",
                                "level": "ERROR",
                                "code": 3002,
                                "content": format!("Cancel order failed: {}", e)
                            }
                        }
                    });

                    let rtn_data = DiffServerMessage::RtnData {
                        data: vec![notify_patch],
                    };

                    ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                    log::warn!("DIFF cancel order failed: order_id={}, error={}", order_id, e);
                }
            }
        } else {
            // OrderRouter 不可用
            let notify_patch = serde_json::json!({
                "notify": {
                    "service_error": {
                        "type": "MESSAGE",
                        "level": "ERROR",
                        "code": 3003,
                        "content": "Order routing service is not available"
                    }
                }
            });

            let rtn_data = DiffServerMessage::RtnData {
                data: vec![notify_patch],
            };

            ctx_addr.do_send(SendDiffMessage { message: rtn_data });
            log::error!("DIFF cancel order failed: OrderRouter not available");
        }
    }

    /// 处理K线订阅请求
    async fn handle_set_chart(
        &self,
        user_id: &str,
        chart_id: String,
        ins_list: String,
        duration: i64,
        view_width: i32,
        ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        // 解析合约列表
        let instruments: Vec<String> = ins_list
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if instruments.is_empty() || ins_list.is_empty() {
            // 空列表表示删除该图表订阅
            let notify_patch = serde_json::json!({
                "notify": {
                    "chart_removed": {
                        "type": "MESSAGE",
                        "level": "INFO",
                        "code": 0,
                        "content": format!("Chart {} removed", chart_id)
                    }
                }
            });

            let rtn_data = DiffServerMessage::RtnData {
                data: vec![notify_patch],
            };

            ctx_addr.do_send(SendDiffMessage { message: rtn_data });
            log::info!("User {} removed chart {}", user_id, chart_id);
            return;
        }

        // 确定周期类型
        let period_name = if duration == 0 {
            "tick"
        } else if duration == 60_000_000_000 {
            "1m"
        } else if duration == 300_000_000_000 {
            "5m"
        } else if duration == 900_000_000_000 {
            "15m"
        } else if duration == 3600_000_000_000 {
            "1h"
        } else if duration == 86400_000_000_000 {
            "1d"
        } else {
            "custom"
        };

        // 发送订阅确认和初始K线数据
        let notify_patch = serde_json::json!({
            "notify": {
                "chart_set": {
                    "type": "MESSAGE",
                    "level": "INFO",
                    "code": 0,
                    "content": format!("Chart {} set for {} instruments ({})", chart_id, instruments.len(), period_name)
                }
            },
            "klines": {
                // 这里应该包含初始K线数据
                // 实际实现中应该从历史数据存储中查询
                instruments[0].clone(): {
                    duration.to_string(): {
                        "last_id": 0,
                        "data": {}
                    }
                }
            }
        });

        let rtn_data = DiffServerMessage::RtnData {
            data: vec![notify_patch],
        };

        ctx_addr.do_send(SendDiffMessage { message: rtn_data });

        log::info!(
            "User {} set chart {}: instruments={:?}, duration={}, view_width={}, period={}",
            user_id,
            chart_id,
            instruments,
            duration,
            view_width,
            period_name
        );

        // TODO: 实际应该：
        // 1. 从历史数据查询最近的K线数据
        // 2. 持续订阅并推送新的K线更新
        // 3. 管理view_width（滚动窗口）
    }

    /// 处理 peek_message（阻塞等待新数据）
    ///
    /// 实现 DIFF 协议的核心同步机制：
    /// 1. 调用 SnapshotManager::peek() 阻塞等待
    /// 2. 收到 patch 后发送 rtn_data 消息
    ///
    /// # 性能特点
    ///
    /// - 零轮询：使用 Tokio Notify 异步等待
    /// - 低延迟：patch 产生后立即唤醒
    /// - 零拷贝：Arc 共享 SnapshotManager
    async fn handle_peek_message(&self, user_id: &str, ctx_addr: Addr<DiffWebsocketSession>) {
        let snapshot_mgr = self.snapshot_mgr.clone();
        let user_id = user_id.to_string();

        // 启动异步任务等待 peek
        tokio::spawn(async move {
            match snapshot_mgr.peek(&user_id).await {
                Some(patches) => {
                    // 收到 patch，发送 rtn_data
                    let rtn_data = DiffServerMessage::RtnData { data: patches };

                    // 发送到 WebSocket session
                    ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                }
                None => {
                    // 超时或用户不存在
                    log::warn!("peek_message timeout for user: {}", user_id);
                }
            }
        });
    }
}

/// DIFF WebSocket 会话
///
/// 集成 SnapshotManager 的 WebSocket 会话
pub struct DiffWebsocketSession {
    /// 会话 ID
    pub session_id: String,

    /// 用户 ID（认证后设置）
    pub user_id: Option<String>,

    /// DIFF 处理器
    pub diff_handler: Arc<DiffHandler>,

    /// 最后心跳时间
    pub heartbeat: std::time::Instant,
}

impl DiffWebsocketSession {
    /// 创建新的 DIFF WebSocket 会话
    pub fn new(session_id: String, diff_handler: Arc<DiffHandler>) -> Self {
        Self {
            session_id,
            user_id: None,
            diff_handler,
            heartbeat: std::time::Instant::now(),
        }
    }

    /// 启动心跳检查
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
        const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if std::time::Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
                log::warn!("DIFF session {} timed out", act.session_id);
                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for DiffWebsocketSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("DIFF WebSocket session {} started", self.session_id);
        self.start_heartbeat(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("DIFF WebSocket session {} stopped", self.session_id);

        // 清理用户快照
        if let Some(ref user_id) = self.user_id {
            let snapshot_mgr = self.diff_handler.snapshot_mgr.clone();
            let user_id = user_id.clone();

            tokio::spawn(async move {
                snapshot_mgr.remove_user(&user_id).await;
            });
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for DiffWebsocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                // 解析 DIFF 协议消息
                match serde_json::from_str::<DiffClientMessage>(&text) {
                    Ok(diff_msg) => {
                        // DIFF 协议消息
                        if let Some(ref user_id) = self.user_id {
                            let handler = self.diff_handler.clone();
                            let user_id = user_id.clone();
                            let ctx_addr = ctx.address();

                            // 异步处理 DIFF 消息
                            ctx.spawn(
                                async move {
                                    handler.handle_diff_message(&user_id, diff_msg, ctx_addr).await;
                                }
                                .into_actor(self),
                            );
                        } else {
                            // 未认证，但允许 ReqLogin 消息
                            if matches!(diff_msg, DiffClientMessage::ReqLogin { .. }) {
                                let handler = self.diff_handler.clone();
                                let user_id = "anonymous".to_string();  // 临时用户ID
                                let ctx_addr = ctx.address();

                                ctx.spawn(
                                    async move {
                                        handler.handle_diff_message(&user_id, diff_msg, ctx_addr).await;
                                    }
                                    .into_actor(self),
                                );
                            } else {
                                log::warn!("Unauthenticated DIFF message from session {}", self.session_id);
                            }
                        }
                    }

                    Err(e) => {
                        log::error!("Failed to parse DIFF message: {}, error: {}", text, e);
                    }
                }
            }

            Ok(ws::Message::Ping(msg)) => {
                self.heartbeat = std::time::Instant::now();
                ctx.pong(&msg);
            }

            Ok(ws::Message::Pong(_)) => {
                self.heartbeat = std::time::Instant::now();
            }

            Ok(ws::Message::Close(reason)) => {
                log::info!("DIFF session {} closing: {:?}", self.session_id, reason);
                ctx.stop();
            }

            _ => {}
        }
    }
}

/// 发送 DIFF 消息到 WebSocket session（Actix 消息）
#[derive(Clone)]
pub struct SendDiffMessage {
    pub message: DiffServerMessage,
}

impl ActixMessage for SendDiffMessage {
    type Result = ();
}

impl ActixHandler<SendDiffMessage> for DiffWebsocketSession {
    type Result = ();

    fn handle(&mut self, msg: SendDiffMessage, ctx: &mut Self::Context) {
        match serde_json::to_string(&msg.message) {
            Ok(json) => {
                ctx.text(json);
            }
            Err(e) => {
                log::error!("Failed to serialize DIFF message: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_handler_creation() {
        use crate::exchange::AccountManager;

        let snapshot_mgr = Arc::new(SnapshotManager::new());
        let account_mgr = Arc::new(AccountManager::new());
        let handler = DiffHandler::new(snapshot_mgr, account_mgr);

        assert!(Arc::strong_count(&handler.snapshot_mgr) >= 1);
    }

    #[tokio::test]
    async fn test_snapshot_manager_integration() {
        let snapshot_mgr = Arc::new(SnapshotManager::new());
        snapshot_mgr.initialize_user("test_user").await;

        // 推送 patch
        let patch = serde_json::json!({
            "balance": 100000.0
        });
        snapshot_mgr.push_patch("test_user", patch).await;

        // peek 应该立即返回
        let patches = snapshot_mgr.peek("test_user").await;
        assert!(patches.is_some());
        assert_eq!(patches.unwrap().len(), 1);
    }
}
