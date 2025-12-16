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

use actix::{
    Actor, ActorContext, Addr, AsyncContext, Context, Handler as ActixHandler,
    Message as ActixMessage, StreamHandler, WrapFuture,
};
use actix_web_actors::ws;
use log;
use std::sync::Arc;
use std::time::Duration;

use super::diff_messages::{DiffClientMessage, DiffServerMessage};
use crate::exchange::{AccountManager, OrderRouter};
use crate::market::{kline_actor::KLineActor, MarketDataBroadcaster};
use crate::protocol::diff::snapshot::SnapshotManager;
use crate::user::UserManager;

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

    /// K线Actor地址（用于查询历史K线）
    pub(crate) kline_actor: Option<Addr<KLineActor>>,
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
            kline_actor: None,
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
    pub fn with_market_broadcaster(
        mut self,
        market_broadcaster: Arc<MarketDataBroadcaster>,
    ) -> Self {
        self.market_broadcaster = Some(market_broadcaster);
        self
    }

    /// 设置K线Actor
    pub fn with_kline_actor(mut self, kline_actor: Addr<KLineActor>) -> Self {
        self.kline_actor = Some(kline_actor);
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

            DiffClientMessage::ReqLogin {
                bid,
                user_name,
                password,
            } => {
                log::info!("DIFF login request: user_name={}, bid={:?}", user_name, bid);
                self.handle_login(user_name, password, ctx_addr).await;
            }

            DiffClientMessage::SubscribeQuote { ins_list } => {
                log::info!("DIFF subscribe quote: ins_list={}", ins_list);
                self.handle_subscribe_quote(user_id, ins_list, ctx_addr)
                    .await;
            }

            DiffClientMessage::InsertOrder {
                user_id: order_user_id,
                account_id, // ✨ 新增字段
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
                log::info!(
                    "DIFF insert order: user_id={}, account_id={:?}, order_id={:?}, time_cond={:?}",
                    order_user_id,
                    account_id,
                    order_id,
                    time_condition
                );
                self.handle_insert_order(
                    user_id,
                    order_user_id,
                    account_id, // ✨ 传递 account_id
                    order_id,
                    exchange_id,
                    instrument_id,
                    direction,
                    offset,
                    volume,
                    price_type,
                    limit_price,
                    time_condition,  // ✨ IOC/FOK/GTC 支持 @yutiansut @quantaxis
                    volume_condition, // ✨ ANY/MIN/ALL 支持 @yutiansut @quantaxis
                    ctx_addr,
                )
                .await;
            }

            DiffClientMessage::CancelOrder {
                user_id: cancel_user_id,
                account_id,
                order_id,
            } => {
                log::info!(
                    "DIFF cancel order: user_id={}, account_id={:?}, order_id={}",
                    cancel_user_id,
                    account_id,
                    order_id
                );
                self.handle_cancel_order(user_id, cancel_user_id, account_id, order_id, ctx_addr)
                    .await; // ✨ 传递 account_id
            }

            DiffClientMessage::SetChart {
                chart_id,
                ins_list,
                duration,
                view_width,
            } => {
                log::info!(
                    "DIFF set chart: chart_id={}, ins_list={}, duration={}, view_width={}",
                    chart_id,
                    ins_list,
                    duration,
                    view_width
                );
                self.handle_set_chart(user_id, chart_id, ins_list, duration, view_width, ctx_addr)
                    .await;
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
            // 尝试两种认证方式：
            // 1. username + password（原始密码）
            // 2. user_id + token（JWT 或其他 token）

            // 先尝试 token 认证（检查 username 是否为 UUID 格式的 user_id）
            let login_result = if username.len() == 36 && username.contains('-') {
                log::info!("Detected UUID format username, attempting token authentication for user_id: {}", username);

                // username 是 UUID 格式，可能是 user_id，尝试 token 认证
                match user_mgr.verify_token(&password) {
                    Ok(verified_user_id) => {
                        log::info!(
                            "Token verification successful, verified_user_id: {}",
                            verified_user_id
                        );

                        // Token 验证成功，检查 user_id 匹配
                        if verified_user_id == username {
                            match user_mgr.get_user(&username) {
                                Ok(user) => {
                                    log::info!("Token auth successful for user: {}", user.username);
                                    // RBAC: 返回角色和权限信息 @yutiansut @quantaxis
                                    let permissions: Vec<String> = user
                                        .get_permissions()
                                        .iter()
                                        .map(|p| format!("{:?}", p))
                                        .collect();
                                    Ok(crate::user::UserLoginResponse {
                                        success: true,
                                        user_id: Some(user.user_id.clone()),
                                        username: Some(user.username.clone()),
                                        token: Some(password.clone()),
                                        message: "Token authentication successful".to_string(),
                                        roles: Some(user.roles.clone()),
                                        is_admin: Some(user.is_admin()),
                                        permissions: Some(permissions),
                                    })
                                }
                                Err(e) => {
                                    log::warn!("Token auth failed: user not found: {}", e);
                                    Ok(crate::user::UserLoginResponse {
                                        success: false,
                                        user_id: None,
                                        username: None,
                                        token: None,
                                        message: format!("User not found: {}", e),
                                        roles: None,
                                        is_admin: None,
                                        permissions: None,
                                    })
                                }
                            }
                        } else {
                            log::warn!(
                                "Token auth failed: user_id mismatch (expected: {}, got: {})",
                                username,
                                verified_user_id
                            );
                            Ok(crate::user::UserLoginResponse {
                                success: false,
                                user_id: None,
                                username: None,
                                token: None,
                                message: "Token user_id mismatch".to_string(),
                                roles: None,
                                is_admin: None,
                                permissions: None,
                            })
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Token verification failed: {}, falling back to password auth",
                            e
                        );
                        // Token 验证失败，尝试常规密码认证（向后兼容）
                        user_mgr.login(crate::user::UserLoginRequest {
                            username: username.clone(),
                            password,
                        })
                    }
                }
            } else {
                log::info!(
                    "Standard password authentication for username: {}",
                    username
                );
                // 常规密码认证
                user_mgr.login(crate::user::UserLoginRequest {
                    username: username.clone(),
                    password,
                })
            };

            match login_result {
                Ok(login_resp) => {
                    if login_resp.success {
                        // 登录成功，初始化用户快照
                        let user_id = login_resp.user_id.clone().unwrap_or_default();
                        self.snapshot_mgr.initialize_user(&user_id).await;

                        // ✅ 设置 session 的 user_id（认证成功）
                        ctx_addr.do_send(SetUserId {
                            user_id: user_id.clone(),
                        });

                        // 发送登录成功通知（通过 SnapshotManager 推送）
                        let notify_patch = serde_json::json!({
                            "notify": {
                                "login_success": {
                                    "type": "MESSAGE",
                                    "level": "INFO",
                                    "code": 0,
                                    "content": format!("Login successful for user: {}", username)
                                }
                            },
                            "user_id": user_id.clone(),
                            "username": username
                        });

                        // ✅ 通过 SnapshotManager 推送（触发 peek_message）
                        self.snapshot_mgr.push_patch(&user_id, notify_patch).await;
                        log::info!(
                            "DIFF login successful: user={}, user_id={}",
                            username,
                            user_id
                        );
                    } else {
                        // 登录失败（也通过 SnapshotManager 推送，确保客户端能收到）
                        let notify_patch = serde_json::json!({
                            "notify": {
                                "login_failed": {
                                    "type": "MESSAGE",
                                    "level": "ERROR",
                                    "code": 1001,
                                    "content": login_resp.message.clone()
                                }
                            }
                        });

                        // 使用 "anonymous" 作为临时 user_id 推送失败消息
                        self.snapshot_mgr
                            .push_patch("anonymous", notify_patch)
                            .await;
                        log::warn!(
                            "DIFF login failed for user: {}, reason: {}",
                            username,
                            login_resp.message
                        );
                    }
                }
                Err(e) => {
                    // 登录错误（也通过 SnapshotManager 推送）
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

                    // 使用 "anonymous" 作为临时 user_id 推送错误消息
                    self.snapshot_mgr
                        .push_patch("anonymous", notify_patch)
                        .await;
                    log::error!("DIFF login error for user {}: {}", username, e);
                }
            }
        } else {
            // UserManager 不可用（也通过 SnapshotManager 推送）
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

            // 使用 "anonymous" 作为临时 user_id 推送错误消息
            self.snapshot_mgr
                .push_patch("anonymous", notify_patch)
                .await;
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

        // 更新用户快照中的合约订阅列表
        self.snapshot_mgr
            .push_patch(
                user_id,
                serde_json::json!({
                    "ins_list": ins_list
                }),
            )
            .await;

        // 发送订阅确认（通过 SnapshotManager 推送，触发 peek_message）
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

        // ✅ 通过 SnapshotManager 推送订阅确认（触发 peek_message 返回）
        self.snapshot_mgr.push_patch(user_id, notify_patch).await;

        log::info!("User {} subscribed to quotes: {:?}", user_id, instruments);

        // ✅ 从 MarketDataBroadcaster 订阅并启动推送任务
        if let Some(ref broadcaster) = self.market_broadcaster {
            let receiver = broadcaster.subscribe(
                user_id.to_string(),
                instruments.clone(),
                vec![
                    "orderbook".to_string(),
                    "tick".to_string(),
                    "last_price".to_string(),
                    "kline".to_string(), // ✨ 新增：订阅K线完成事件
                ],
            );

            // 启动异步任务持续推送行情数据
            let snapshot_mgr = self.snapshot_mgr.clone();
            let user_id_clone = user_id.to_string();

            tokio::spawn(async move {
                log::info!(
                    "Started market data streaming task for user: {}",
                    user_id_clone
                );

                loop {
                    // ✅ 使用 spawn_blocking 避免阻塞 Tokio 执行器
                    let receiver_clone = receiver.clone();
                    match tokio::task::spawn_blocking(move || receiver_clone.recv()).await {
                        Ok(Ok(event)) => {
                            // 将 MarketDataEvent 转换为 DIFF quote 格式
                            if let Some(quote_patch) = Self::convert_market_event_to_diff(&event) {
                                snapshot_mgr.push_patch(&user_id_clone, quote_patch).await;
                                log::debug!("Pushed market data patch for user: {}", user_id_clone);
                            }
                        }
                        Ok(Err(_)) => {
                            log::warn!(
                                "Market data channel disconnected for user: {}",
                                user_id_clone
                            );
                            break;
                        }
                        Err(e) => {
                            log::error!("spawn_blocking error: {}", e);
                            break;
                        }
                    }
                }

                log::info!(
                    "Market data streaming task ended for user: {}",
                    user_id_clone
                );
            });
        } else {
            log::warn!("MarketDataBroadcaster not available, skipping live market data");
        }
    }

    /// 处理下单请求
    async fn handle_insert_order(
        &self,
        session_user_id: &str,
        order_user_id: String,
        client_account_id: Option<String>, // ✨ 新增参数
        order_id: Option<String>,          // ✅ 修改为 Option<String>
        exchange_id: String,
        instrument_id: String,
        direction: String,
        offset: String,
        volume: i64,
        price_type: String,
        limit_price: Option<f64>,
        time_condition: Option<String>,    // ✨ IOC/FOK/GTC 支持 @yutiansut @quantaxis
        volume_condition: Option<String>,  // ✨ ANY/MIN/ALL 支持 @yutiansut @quantaxis
        ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        // ✅ 自动生成 order_id（如果客户端未提供）
        let order_id = order_id.unwrap_or_else(|| {
            let id = uuid::Uuid::new_v4().to_string();
            log::debug!("Auto-generated order_id: {}", id);
            id
        });

        // ✨ 修改验证逻辑：交易所只关心account_id，不验证user_id
        //
        // 如果客户端提供了account_id，则只验证账户权限（不验证user_id匹配）
        // 原因：交易所视角只认账户，User→Account映射是经纪商业务 @yutiansut @quantaxis
        if client_account_id.is_none() {
            // 向后兼容：如果没有提供account_id，则验证user_id匹配
            if session_user_id != order_user_id {
                let notify_patch = serde_json::json!({
                    "notify": {
                        "order_error": {
                            "type": "MESSAGE",
                            "level": "ERROR",
                            "code": 2001,
                            "content": "User ID mismatch (legacy mode: provide account_id to bypass)"
                        }
                    }
                });

                let rtn_data = DiffServerMessage::RtnData {
                    data: vec![notify_patch],
                };

                ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                log::warn!(
                    "DIFF insert order failed: user mismatch (session={}, order={}) - legacy mode",
                    session_user_id,
                    order_user_id
                );
                return;
            }
        } else {
            log::debug!(
                "DIFF insert order: skipping user_id verification (account_id provided: {:?})",
                client_account_id
            );
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
                if let Err(e) = self
                    .account_mgr
                    .verify_account_ownership(acc_id, &order_user_id)
                {
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
                    log::warn!(
                        "DIFF insert order failed: account verification failed for user {}: {}",
                        order_user_id,
                        e
                    );
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
                            log::warn!(
                                "DIFF insert order failed: no accounts found for user {}",
                                order_user_id
                            );
                            return;
                        }
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
            // ✨ 支持 IOC/FOK/GTC 时间条件 和 ANY/MIN/ALL 数量条件 @yutiansut @quantaxis
            use crate::exchange::order_router::{TimeCondition, VolumeCondition};
            let time_cond = time_condition.as_ref()
                .map(|s| TimeCondition::from_str(s));
            let volume_cond = volume_condition.as_ref()
                .map(|s| VolumeCondition::from_str(s));

            let req = crate::exchange::order_router::SubmitOrderRequest {
                account_id,
                instrument_id: instrument_id.clone(),
                direction: direction.clone(),
                offset: offset.clone(),
                volume: volume as f64,
                price: limit_price.unwrap_or(0.0),
                order_type: order_type.to_string(),
                time_condition: time_cond,
                volume_condition: volume_cond,
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
        client_account_id: Option<String>, // ✨ 新增参数
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
                if let Err(e) = self
                    .account_mgr
                    .verify_account_ownership(acc_id, &cancel_user_id)
                {
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
                    log::warn!(
                        "DIFF cancel order failed: account verification failed for user {}: {}",
                        cancel_user_id,
                        e
                    );
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
                            log::warn!(
                                "DIFF cancel order failed: no accounts found for user {}",
                                cancel_user_id
                            );
                            return;
                        }
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
                account_id, // 交易层只关心 account_id
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
                    log::warn!(
                        "DIFF cancel order failed: order_id={}, error={}",
                        order_id,
                        e
                    );
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

    /// 处理K线订阅请求（DIFF协议 set_chart）
    /// ✨ 增强调试日志 @yutiansut @quantaxis
    async fn handle_set_chart(
        &self,
        user_id: &str,
        chart_id: String,
        ins_list: String,
        duration: i64,
        view_width: i32,
        _ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        log::info!(
            "📊 [DIFF set_chart] Received request: user={}, chart_id={}, ins_list={}, duration={}, view_width={}",
            user_id, chart_id, ins_list, duration, view_width
        );

        // 解析合约列表
        let instruments: Vec<String> = ins_list
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        log::info!(
            "📊 [DIFF set_chart] Parsed instruments: {:?}",
            instruments
        );

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

            self.snapshot_mgr.push_patch(user_id, notify_patch).await;
            log::info!("User {} removed chart {}", user_id, chart_id);
            return;
        }

        // 转换duration(ns) 到 KLinePeriod
        let period = crate::market::kline::KLinePeriod::from_duration_ns(duration);

        if period.is_none() {
            // 不支持的周期
            let notify_patch = serde_json::json!({
                "notify": {
                    "chart_error": {
                        "type": "MESSAGE",
                        "level": "ERROR",
                        "code": 5001,
                        "content": format!("Unsupported duration: {}", duration)
                    }
                }
            });

            self.snapshot_mgr.push_patch(user_id, notify_patch).await;
            log::warn!("Unsupported K-line duration: {}", duration);
            return;
        }

        let period = period.unwrap();
        let period_name = format!("{:?}", period);

        log::info!(
            "📊 [DIFF set_chart] Converted period: {:?} (period_name={})",
            period, period_name
        );

        // 查询历史K线数据（从KLineActor）
        if let Some(ref kline_actor) = self.kline_actor {
            log::info!("📊 [DIFF set_chart] KLineActor is available, querying K-lines...");
            // 取第一个合约作为主合约
            let instrument_id = instruments[0].clone();
            let count = view_width.max(100) as usize; // 至少100根

            // 异步查询K线
            let kline_actor_clone = kline_actor.clone();
            let snapshot_mgr = self.snapshot_mgr.clone();
            let user_id_str = user_id.to_string();
            let chart_id_clone = chart_id.clone();
            let instruments_clone = instruments.clone();

            tokio::spawn(async move {
                log::info!(
                    "📊 [DIFF set_chart] Querying KLineActor: instrument={}, period={:?}, count={}",
                    instrument_id, period, count
                );

                // 查询历史K线
                let klines = kline_actor_clone
                    .send(crate::market::GetKLines {
                        instrument_id: instrument_id.clone(),
                        period,
                        count,
                    })
                    .await;

                match klines {
                    Ok(klines) => {
                        log::info!(
                            "📊 [DIFF set_chart] KLineActor returned {} K-lines for {}",
                            klines.len(), instrument_id
                        );

                        // 如果没有真实K线数据，生成模拟数据用于测试 @yutiansut @quantaxis
                        let klines = if klines.is_empty() {
                            log::info!(
                                "📊 [DIFF set_chart] No real K-line data, generating mock data for testing"
                            );
                            generate_mock_klines(&instrument_id, period, count)
                        } else {
                            klines
                        };

                        // 转换为DIFF格式
                        let mut kline_data = serde_json::Map::new();
                        let mut last_kline_id = 0i64;

                        for kline in klines.iter() {
                            // K线ID使用时间戳除以周期得到序列号
                            let kline_id = (kline.timestamp * 1_000_000) / duration; // 毫秒转纳秒后除以周期

                            // DIFF协议要求datetime为UnixNano（纳秒）
                            let datetime_ns = kline.timestamp * 1_000_000; // 毫秒转纳秒

                            kline_data.insert(
                                kline_id.to_string(),
                                serde_json::json!({
                                    "datetime": datetime_ns,
                                    "open": kline.open,
                                    "high": kline.high,
                                    "low": kline.low,
                                    "close": kline.close,
                                    "volume": kline.volume,
                                    "open_oi": kline.open_oi,
                                    "close_oi": kline.close_oi,
                                }),
                            );
                            last_kline_id = kline_id;
                        }

                        // 发送K线数据
                        let kline_patch = serde_json::json!({
                            "notify": {
                                "chart_set": {
                                    "type": "MESSAGE",
                                    "level": "INFO",
                                    "code": 0,
                                    "content": format!("Chart {} set for {} instruments", chart_id_clone, instruments_clone.len())
                                }
                            },
                            "klines": {
                                instrument_id.clone(): {
                                    duration.to_string(): {
                                        "last_id": last_kline_id,
                                        "data": kline_data
                                    }
                                }
                            }
                        });

                        snapshot_mgr.push_patch(&user_id_str, kline_patch).await;
                        log::info!(
                            "📊 [DIFF] User {} set chart {}: instrument={}, period={:?}, bars={}",
                            user_id_str,
                            chart_id_clone,
                            instrument_id,
                            period,
                            klines.len()
                        );

                        // 订阅实时K线更新（通过MarketDataBroadcaster的kline频道）
                        // 注意：已经在 handle_subscribe_quote 中订阅了 kline 频道，这里无需重复订阅
                        // 如果需要独立的K线订阅，可以在下面实现
                    }
                    Err(e) => {
                        let error_patch = serde_json::json!({
                            "notify": {
                                "chart_error": {
                                    "type": "MESSAGE",
                                    "level": "ERROR",
                                    "code": 5002,
                                    "content": format!("Failed to fetch K-line data: {}", e)
                                }
                            }
                        });

                        snapshot_mgr.push_patch(&user_id_str, error_patch).await;
                        log::error!(
                            "Failed to fetch K-line data for chart {}: {}",
                            chart_id_clone,
                            e
                        );
                    }
                }
            });
        } else {
            // KLineActor 不可用
            let notify_patch = serde_json::json!({
                "notify": {
                    "service_error": {
                        "type": "MESSAGE",
                        "level": "ERROR",
                        "code": 5003,
                        "content": "K-line service is not available"
                    }
                }
            });

            self.snapshot_mgr.push_patch(user_id, notify_patch).await;
            log::error!("K-line service not available for set_chart request");
        }
    }

    /// 将 MarketDataEvent 转换为 DIFF 格式的 JSON patch
    fn convert_market_event_to_diff(
        event: &crate::market::MarketDataEvent,
    ) -> Option<serde_json::Value> {
        use crate::market::MarketDataEvent;

        match event {
            MarketDataEvent::OrderBookSnapshot {
                instrument_id,
                bids,
                asks,
                timestamp,
            } => {
                // 转换为 DIFF quotes 格式
                Some(serde_json::json!({
                    "quotes": {
                        instrument_id: {
                            "instrument_id": instrument_id,
                            "datetime": timestamp,
                            "bid_price1": bids.get(0).map(|b| b.price),
                            "bid_volume1": bids.get(0).map(|b| b.volume),
                            "ask_price1": asks.get(0).map(|a| a.price),
                            "ask_volume1": asks.get(0).map(|a| a.volume),
                        }
                    }
                }))
            }

            MarketDataEvent::LastPrice {
                instrument_id,
                price,
                timestamp,
            } => Some(serde_json::json!({
                "quotes": {
                    instrument_id: {
                        "instrument_id": instrument_id,
                        "last_price": price,
                        "datetime": timestamp,
                    }
                }
            })),

            MarketDataEvent::Tick {
                instrument_id,
                price,
                volume,
                direction: _,
                timestamp,
            } => Some(serde_json::json!({
                "quotes": {
                    instrument_id: {
                        "instrument_id": instrument_id,
                        "last_price": price,
                        "volume": volume,
                        "datetime": timestamp,
                    }
                }
            })),

            MarketDataEvent::OrderBookUpdate {
                instrument_id,
                side,
                price,
                volume,
                timestamp,
            } => {
                // 增量更新转换为完整字段更新
                if side == "bid" {
                    Some(serde_json::json!({
                        "quotes": {
                            instrument_id: {
                                "instrument_id": instrument_id,
                                "bid_price1": price,
                                "bid_volume1": volume,
                                "datetime": timestamp,
                            }
                        }
                    }))
                } else {
                    Some(serde_json::json!({
                        "quotes": {
                            instrument_id: {
                                "instrument_id": instrument_id,
                                "ask_price1": price,
                                "ask_volume1": volume,
                                "datetime": timestamp,
                            }
                        }
                    }))
                }
            }

            MarketDataEvent::KLineFinished {
                instrument_id,
                period,
                kline,
                timestamp: _,
            } => {
                // 转换为 DIFF klines 格式（增量推送新K线）
                let duration_ns = crate::market::kline::KLinePeriod::from_int(*period)
                    .map(|p| p.to_duration_ns())
                    .unwrap_or(0);

                // K线ID使用时间戳除以周期得到序列号
                let kline_id = (kline.timestamp * 1_000_000) / duration_ns; // 毫秒转纳秒后除以周期

                // DIFF协议要求datetime为UnixNano（纳秒）
                let datetime_ns = kline.timestamp * 1_000_000; // 毫秒转纳秒

                Some(serde_json::json!({
                    "klines": {
                        instrument_id: {
                            duration_ns.to_string(): {
                                "data": {
                                    kline_id.to_string(): {
                                        "datetime": datetime_ns,
                                        "open": kline.open,
                                        "high": kline.high,
                                        "low": kline.low,
                                        "close": kline.close,
                                        "volume": kline.volume,
                                        "open_oi": kline.open_oi,
                                        "close_oi": kline.close_oi,
                                    }
                                }
                            }
                        }
                    }
                }))
            }

            MarketDataEvent::FactorUpdate {
                instrument_id,
                factors,
                period,
                timestamp,
            } => {
                // 转换为 DIFF factors 格式
                // factors 字段包含计算完成的因子值
                Some(serde_json::json!({
                    "factors": {
                        instrument_id: {
                            "period": period,
                            "timestamp": timestamp,
                            "values": factors,
                        }
                    }
                }))
            }
        }
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
                                    handler
                                        .handle_diff_message(&user_id, diff_msg, ctx_addr)
                                        .await;
                                }
                                .into_actor(self),
                            );
                        } else {
                            // 未认证，只允许 ReqLogin 消息
                            if matches!(diff_msg, DiffClientMessage::ReqLogin { .. }) {
                                let handler = self.diff_handler.clone();
                                let user_id = "anonymous".to_string(); // 临时用户ID
                                let ctx_addr = ctx.address();

                                ctx.spawn(
                                    async move {
                                        handler
                                            .handle_diff_message(&user_id, diff_msg, ctx_addr)
                                            .await;
                                    }
                                    .into_actor(self),
                                );
                            } else {
                                log::warn!(
                                    "Unauthenticated DIFF message from session {}, rejecting: {:?}",
                                    self.session_id,
                                    diff_msg
                                );
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

/// 设置 session 的 user_id（Actix 消息）
#[derive(Clone)]
pub struct SetUserId {
    pub user_id: String,
}

impl ActixMessage for SetUserId {
    type Result = ();
}

impl ActixHandler<SetUserId> for DiffWebsocketSession {
    type Result = ();

    fn handle(&mut self, msg: SetUserId, _ctx: &mut Self::Context) {
        self.user_id = Some(msg.user_id.clone());
        log::info!(
            "Session {} authenticated as user {}",
            self.session_id,
            msg.user_id
        );
    }
}

/// 生成模拟K线数据用于测试 @yutiansut @quantaxis
///
/// 当没有真实交易数据时，生成模拟K线供前端测试图表渲染
fn generate_mock_klines(
    instrument_id: &str,
    period: crate::market::kline::KLinePeriod,
    count: usize,
) -> Vec<crate::market::kline::KLine> {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let mut klines = Vec::with_capacity(count);

    // 根据合约类型确定基准价格
    let base_price = if instrument_id.contains("IF") || instrument_id.contains("IC") || instrument_id.contains("IM") || instrument_id.contains("IH") {
        // 股指期货
        if instrument_id.contains("IF") {
            4500.0
        } else if instrument_id.contains("IH") {
            3000.0
        } else if instrument_id.contains("IC") || instrument_id.contains("IM") {
            7000.0
        } else {
            4000.0
        }
    } else if instrument_id.contains("au") {
        // 黄金
        900.0
    } else if instrument_id.contains("cu") {
        // 铜
        85000.0
    } else if instrument_id.contains("sc") {
        // 原油
        450.0
    } else if instrument_id.contains("T") || instrument_id.contains("TF") || instrument_id.contains("TL") {
        // 国债期货
        108.0
    } else if instrument_id.contains("i") {
        // 铁矿石
        780.0
    } else {
        // 默认价格
        3000.0
    };

    // 周期对应的毫秒数
    let period_ms: i64 = match period {
        crate::market::kline::KLinePeriod::Sec3 => 3 * 1000,
        crate::market::kline::KLinePeriod::Min1 => 60 * 1000,
        crate::market::kline::KLinePeriod::Min5 => 5 * 60 * 1000,
        crate::market::kline::KLinePeriod::Min15 => 15 * 60 * 1000,
        crate::market::kline::KLinePeriod::Min30 => 30 * 60 * 1000,
        crate::market::kline::KLinePeriod::Min60 => 60 * 60 * 1000,
        crate::market::kline::KLinePeriod::Day => 24 * 60 * 60 * 1000,
    };

    // 从当前时间往前生成count根K线
    let now_ms = chrono::Utc::now().timestamp_millis();
    let start_ms = now_ms - (count as i64) * period_ms;

    let mut current_price: f64 = base_price;
    let volatility: f64 = base_price * 0.002; // 0.2%波动率

    for i in 0..count {
        let timestamp = start_ms + (i as i64) * period_ms;

        // 随机游走生成OHLC
        let change: f64 = rng.gen_range(-volatility..volatility);
        let open: f64 = current_price;
        let close: f64 = f64::max(f64::min(current_price + change, base_price * 1.1), base_price * 0.9);

        // high和low在open和close范围内随机扩展
        let spread: f64 = (open - close).abs() * rng.gen_range(0.2f64..1.5f64);
        let high: f64 = f64::max(open, close) + spread * rng.gen_range(0.0f64..1.0f64);
        let low: f64 = f64::min(open, close) - spread * rng.gen_range(0.0f64..1.0f64);

        let volume = rng.gen_range(100..5000) as i64;
        let amount = close * volume as f64;

        klines.push(crate::market::kline::KLine {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
            amount,
            open_oi: 10000 + rng.gen_range(-500..500) as i64,
            close_oi: 10000 + rng.gen_range(-500..500) as i64,
            is_finished: true,
        });

        current_price = close;
    }

    log::info!(
        "📊 [MockKLines] Generated {} mock K-lines for {} {:?}, price range: {:.2}~{:.2}",
        klines.len(),
        instrument_id,
        period,
        klines.iter().map(|k| k.low).fold(f64::INFINITY, f64::min),
        klines.iter().map(|k| k.high).fold(f64::NEG_INFINITY, f64::max)
    );

    klines
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
