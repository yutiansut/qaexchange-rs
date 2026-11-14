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
use crate::core::account_ext::AccountExt;   // safe_risk_ratio @yutiansut @quantaxis
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

    /// 行情服务 —— 仅用于**订阅时补一次初始快照**。@yutiansut @quantaxis
    ///
    /// 为什么需要:`MarketDataBroadcaster` 只做订阅分发,**不缓存行情**;
    /// 而 DIFF 的 quote 只有在 Tick/LastPrice **事件发生**时才带
    /// `last_price`/`volume`。市场空闲(实测 10 秒 0 成交)时这两类事件不产生,
    /// 客户端订阅后**在下一笔成交之前永远拿不到最新价与成交量** ——
    /// 而 HTTP 侧 `/api/market/tick/{id}` 同一时刻是有值的(实测 last_price=2525.8)。
    pub(crate) market_data_service: Option<Arc<crate::market::MarketDataService>>,

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
            market_data_service: None,
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

    /// 注入行情服务(用于订阅时的初始快照)。@yutiansut @quantaxis
    pub fn with_market_data_service(
        mut self,
        market_data_service: Arc<crate::market::MarketDataService>,
    ) -> Self {
        self.market_data_service = Some(market_data_service);
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
    /// * `conn_id` - 连接 ID(WebSocket session_id):peek、登录结果、行情、K 线都按连接走
    /// * `user_id` - 会话用户(`?user_id=` 或登录后的 user_id),用于下单/撤单的归属校验
    /// * `msg` - DIFF 客户端消息
    /// * `ctx` - WebSocket 上下文
    pub async fn handle_diff_message(
        &self,
        conn_id: &str,
        user_id: &str,
        msg: DiffClientMessage,
        ctx_addr: Addr<DiffWebsocketSession>,
    ) {
        match msg {
            DiffClientMessage::PeekMessage => {
                self.handle_peek_message(conn_id, ctx_addr).await;
            }

            DiffClientMessage::ReqLogin {
                bid,
                user_name,
                password,
            } => {
                log::info!("DIFF login request: user_name={}, bid={:?}", user_name, bid);
                self.handle_login(conn_id, user_name, password, ctx_addr).await;
            }

            DiffClientMessage::SubscribeQuote { ins_list } => {
                log::info!("DIFF subscribe quote: ins_list={}", ins_list);
                self.handle_subscribe_quote(conn_id, user_id, ins_list, ctx_addr)
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
                self.handle_set_chart(conn_id, user_id, chart_id, ins_list, duration, view_width, ctx_addr)
                    .await;
            }

            DiffClientMessage::Ping => {
                // 心跳响应: 收到 ping 立即返回 pong @yutiansut @quantaxis
                log::debug!("DIFF ping received from user={}, sending pong", user_id);
                let response = DiffServerMessage::Pong;
                ctx_addr.do_send(SendDiffMessage { message: response });
            }
        }
    }

    /// 处理登录请求
    async fn handle_login(
        &self,
        conn_id: &str,
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
                        // 登录成功:本连接绑定到该用户的主题(交易回报按主题扇出给每条连接)。
                        // ⚠️ 必须**先绑定、再构造下面的初始快照**:之间发生的回报经主题进本连接,
                        // 初始快照随后覆盖成最新状态;反过来就会漏掉这段时间的回报。@yutiansut @quantaxis
                        let user_id = login_resp.user_id.clone().unwrap_or_default();
                        self.snapshot_mgr.bind_topic(conn_id, &user_id);

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

                        // ✅ 只推给本连接(同一用户的其他连接不该收到这条登录回包)
                        self.snapshot_mgr.push_to_connection(conn_id, notify_patch);

                        // ✨ 登录后补一次**账户初始快照**。@yutiansut @quantaxis
                        //
                        // DIFF 的约定是登录后下发完整业务快照,而此前登录 patch 只有
                        // `notify` / `user_id` / `username` —— **没有 accounts** ——
                        // 于是客户端在**下一次成交或撤单之前拿不到任何账户数据**
                        // (账户 patch 只由 `trade_gateway::push_account_update` 在
                        //  成交/撤单时产生)。行情侧的同一个问题已经修过
                        // (`handle_subscribe_quote` 的 `initial_quotes`,见 :496),
                        // 账户侧一直没补。`DiffHandler` 本来就持有 `account_mgr`(:51),
                        // 依赖是齐的,只差这一根线。
                        //
                        // 字段集与增量推送(`trade_gateway.rs:1775`)**保持一致** ——
                        // 否则首帧与后续帧形状不同,客户端合并出来的对象会时有时无字段。
                        {
                            let mut accounts_patch = serde_json::Map::new();
                            for account in self.account_mgr.get_accounts_by_user(&user_id) {
                                let mut acc = account.write();
                                let account_id = acc.account_cookie.clone();
                                let margin = acc.get_margin() + acc.get_frozen_margin();
                                accounts_patch.insert(
                                    account_id,
                                    serde_json::json!({
                                        "balance": acc.get_balance(),
                                        "available": acc.money,
                                        "margin": margin,
                                        "position_profit": acc.get_positionprofit(),
                                        "risk_ratio": acc.safe_risk_ratio(),
                                    }),
                                );
                            }
                            if !accounts_patch.is_empty() {
                                let n = accounts_patch.len();
                                self.snapshot_mgr.push_to_connection(
                                    conn_id,
                                    serde_json::json!({ "accounts": accounts_patch }),
                                );
                                log::info!(
                                    "DIFF 登录后补发账户初始快照: user={}, {} 个账户",
                                    user_id, n
                                );
                            }
                        }

                        // ✨ DIFF 标准初始快照:名下每个账户的 `trade.<account>.{accounts,positions}`
                        //    + 活单 `orders`,**只推给本连接**。交易回报只推给有连接在听的主题,
                        //    所以新连接的当前状态必须在这里补齐。按账户 ID 连接的会话
                        //    (user_id 本身就是账户)同样补该账户。@yutiansut @quantaxis
                        if let Some(ref router) = self.order_router {
                            let mut account_ids: Vec<String> = self
                                .account_mgr
                                .get_accounts_by_user(&user_id)
                                .iter()
                                .map(|a| a.read().account_cookie.clone())
                                .collect();
                            if account_ids.is_empty() && self.account_mgr.get_account(&user_id).is_ok() {
                                account_ids.push(user_id.clone());
                            }
                            let gw = router.trade_gateway();
                            for acc_id in &account_ids {
                                gw.publish_account_views_to(Some(conn_id), acc_id);
                                for (key, view) in router.diff_live_order_views(acc_id) {
                                    gw.publish_trade_section_to(Some(conn_id), acc_id, "orders", &key, view);
                                }
                            }
                            log::info!(
                                "DIFF 登录后补发标准交易截面: user={}, 账户 {:?}",
                                user_id, account_ids
                            );
                        }
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

                        // 只推给本连接(原先推给共享的 "anonymous" 截面,任何未登录连接都可能把它取走)
                        self.snapshot_mgr.push_to_connection(conn_id, notify_patch);
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

                    self.snapshot_mgr.push_to_connection(conn_id, notify_patch);
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

            self.snapshot_mgr.push_to_connection(conn_id, notify_patch);
            log::error!("DIFF login failed: UserManager not available");
        }
    }

    /// 处理行情订阅请求
    async fn handle_subscribe_quote(
        &self,
        conn_id: &str,
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

        // 更新本连接截面中的合约订阅列表(订阅是**按连接**的:同一用户的两条连接各订各的)
        self.snapshot_mgr.push_to_connection(
            conn_id,
            serde_json::json!({
                "ins_list": ins_list
            }),
        );

        // 发送订阅确认（通过 SnapshotManager 推送，触发 peek_message）
        // ✨ 订阅时补一次**初始行情快照**。@yutiansut @quantaxis
        //
        // 原来这里是空对象,注释写着「实际实现中应该从 MarketDataBroadcaster 获取当前快照」——
        // 而 broadcaster 只做分发、不缓存行情。后果(2026-09-05 实测):
        // DIFF 的 quote 只有在 Tick/LastPrice **事件发生**时才带 last_price/volume,
        // 市场空闲(10 秒 0 成交)这两类事件不产生,客户端订阅后
        // **在下一笔成交之前永远拿不到最新价与成交量**;
        // 前端因此显示"成交量 0"(而 HTTP 侧同一时刻是 1),点切到 HTTP 轮询才对。
        //
        // 用的是 HTTP 侧已在用、已验证的 `get_tick_data`(market/mod.rs:430),
        // **不新写取数逻辑**。取不到就跳过该合约(不编造 0)——
        // 拿不到数据时宁可不给字段,让前端显示 `--`。
        //
        // ✨ 2026-11-08:初始快照改为与实时广播**同一份完整快照**(`quote_snapshot` +
        // `quote_value`),而不是 tick 的三个字段 —— 否则订阅后的第一帧没有盘口档位、
        // `volume` 还是最后一笔的单笔量。@yutiansut @quantaxis
        let initial_quotes = {
            let mut q = serde_json::Map::new();
            if let Some(ref mds) = self.market_data_service {
                for inst in &instruments {
                    if let Ok((book, stats)) = mds.quote_snapshot(inst, 10) {
                        q.insert(
                            inst.clone(),
                            Self::quote_value(inst, &book.bids, &book.asks, book.timestamp, Some(&stats)),
                        );
                    }
                }
            }
            serde_json::Value::Object(q)
        };

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
            "quotes": initial_quotes
        });

        // ✅ 订阅确认 + 初始行情只推给本连接（触发 peek_message 返回）
        self.snapshot_mgr.push_to_connection(conn_id, notify_patch);

        log::info!("User {} subscribed to quotes: {:?}", user_id, instruments);

        // ✅ 从 MarketDataBroadcaster 订阅并启动推送任务
        // ⚠️ 订阅者 ID 用**连接 ID**:原先用 user_id,同一用户的第二条连接订阅时会顶掉第一条的
        //    行情流(广播器按 ID 覆盖),任一条断开又会把同用户的订阅一起注销。@yutiansut @quantaxis
        if let Some(ref broadcaster) = self.market_broadcaster {
            let receiver = broadcaster.subscribe(
                conn_id.to_string(),
                instruments.clone(),
                vec![
                    // quotes 来自实时完整快照(orderbook 频道,事件驱动、10ms 合并,
                    // 见 market/snapshot_broadcaster.rs);逐笔 tick / last_price 已体现在
                    // 快照的 last_price / volume / amount 里,不再单独转成 quote 补丁。
                    // ⚠️ K 线 / 因子仍要订 —— 它们是独立频道,与此无关。@yutiansut @quantaxis
                    "orderbook".to_string(),
                    // ✨ 频道名必须与 broadcaster.rs:470 发出的完全一致 —— 过滤是精确
                    // 字符串相等 (broadcaster.rs:493 `ch == channel`),写成 "kline"
                    // 永远匹配不上,DIFF 客户端一根 K 线都收不到。@yutiansut @quantaxis
                    "kline_finished".to_string(),
                    // ✨ 补上 "factor" —— 上一次修频道名时只改了 kline,漏了这个。
                    // 后果:`convert_market_event_to_diff` 里明明有 FactorUpdate 分支
                    // (:1315 产出 `{"factors": {...}}`),KLineActor 也确实在算
                    // (debug 日志可见 `Factor update for IF2501: {"ma5":2526.2,...}`),
                    // 但事件在**广播器的频道过滤**这一层就被丢掉了 ——
                    // 过滤是精确字符串相等,列表里没有 "factor" 就一条都不发。
                    // 前端 `/factors` 读 `snapshot.factors` 因此永远是空。
                    //
                    // 实测(补之前):WS 收到 127 条 rtn_data,顶层键只有
                    // ["ins_list","klines","notify","quotes"],没有 factors。
                    // @yutiansut @quantaxis
                    "factor".to_string(),
                ],
            );

            // 启动异步任务持续推送行情数据
            let snapshot_mgr = self.snapshot_mgr.clone();
            let user_id_clone = user_id.to_string();
            let conn_id_clone = conn_id.to_string();

            tokio::spawn(async move {
                log::info!(
                    "Started market data streaming task for user: {}",
                    user_id_clone
                );

                // ✨ 定时非阻塞轮询,**不要**用 spawn_blocking(recv()) @yutiansut @quantaxis
                //
                // 原写法是 `spawn_blocking(move || receiver.recv())`,注释还写着
                // 「避免阻塞 Tokio 执行器」—— 但 `recv()` 会一直阻塞到有行情事件为止,
                // 而阻塞中的线程**不算 idle,tokio 阻塞池永远不会回收它**。
                // 效果是「每个订阅者永久占用一个 OS 线程」:
                //   · 实测压测后 `actix-rt` 线程 24 → 120,空载 405 秒纹丝不动;
                //   · 每个线程还带一份 mimalloc 本地 segment,直接抵消 worker 限流的收益;
                //   · 并发订阅者被 tokio 阻塞池上限(默认 512)硬卡住。
                //
                // 本仓库的另一条 WS 路径 `session.rs:140 start_market_data_listener`
                // 早就做对了:`run_interval(10ms)` + `try_recv()` 批量取 + 背压丢弃。
                // 这里照搬同一套语义,只是把 actix 的 run_interval 换成 tokio interval。
                let mut ticker = tokio::time::interval(std::time::Duration::from_millis(10));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                const MAX_BATCH: usize = 100;
                'outer: loop {
                    ticker.tick().await;

                    // 背压:积压过多时先丢一半旧事件,与 session.rs 的阈值保持一致
                    let queue_len = receiver.len();
                    if queue_len > 500 {
                        for _ in 0..(queue_len / 2) {
                            if receiver.try_recv().is_err() {
                                break;
                            }
                        }
                        log::warn!(
                            "Market data backlog {} for user {}, dropped half",
                            queue_len,
                            user_id_clone
                        );
                    }

                    for _ in 0..MAX_BATCH {
                        match receiver.try_recv() {
                            Ok(event) => {
                                if let Some(quote_patch) = Self::convert_market_event_to_diff(&event)
                                {
                                    // 连接已注销就结束任务(不依赖 stopped() 里的注销先到)
                                    if !snapshot_mgr.push_to_connection(&conn_id_clone, quote_patch) {
                                        break 'outer;
                                    }
                                }
                            }
                            Err(crossbeam::channel::TryRecvError::Empty) => break,
                            Err(crossbeam::channel::TryRecvError::Disconnected) => {
                                // 降为 debug —— 这是**正常的连接关闭路径**,不是异常
                                // @yutiansut @quantaxis
                                //
                                // 每个 WS 连接断开都会走到这里,于是生产日志里
                                // 370 条 WARN **全部**是这一条(实测 21 小时,
                                // 与访问日志的 216 次 101 升级量级吻合)。
                                // 而紧接着 :578 已经用 info! 记了「任务结束」——
                                // **同一件事记了两条,一条 warn 一条 info**。
                                //
                                // 危害不在磁盘,在于**它把 WARN 这一级污染成了噪声**:
                                // 真正需要注意的告警(如本会话新加的保证金率不一致)
                                // 会被 370 条正常断连淹没。
                                log::debug!(
                                    "Market data channel disconnected for user: {}",
                                    user_id_clone
                                );
                                break 'outer;
                            }
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
        // ✨ 客户端给的 order_id 是 DIFF 回报的键(trade.<account>.orders.<order_id>),
        //    原样透传给路由;没给就由交易所单号做键 @yutiansut @quantaxis
        let client_order_id = order_id.clone();
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

            // 提交订单(带客户端编号)
            let req_for_reject = req.clone();
            let response = order_router.submit_order_with_client_id(req, client_order_id.clone());

            if response.success {
                // ✨ 不再直发一份「下单确认」:订单视图已由路由按 DIFF 标准推入
                //    `trade.<account>.orders.<order_id>`(ALIVE/FINISHED + status_code + volume_left)。
                //    原先这里另推 `trade.<user_id>.orders.<order_id>`,键在用户而非账户、status
                //    用的是撮合层小写词,与回报补丁是两份不同结构 @yutiansut @quantaxis
                log::info!(
                    "DIFF insert order success: order_id={} exchange_order_id={:?}",
                    order_id,
                    response.order_id
                );
            } else {
                // 柜台层拒单:推一张终态订单视图(FINISHED + status_code 7 + error_id,
                // exchange_order_id 为空);FOK 未满足报 status_code 4 —— 见 publish_rejected_order
                order_router.publish_rejected_order(
                    &req_for_reject,
                    client_order_id.as_deref(),
                    &exchange_id,
                    &response,
                );
                let code = response.error_code.unwrap_or(2002);
                let content = response.error_message.unwrap_or("Order rejected".to_string());
                let error_id = crate::exchange::order_router::ctp_error_id(code, &content);
                let notify_patch = serde_json::json!({
                    "notify": {
                        "order_rejected": {
                            "type": "MESSAGE",
                            "level": "ERROR",
                            "code": code,
                            "error_id": error_id,
                            "order_id": order_id,
                            "content": content
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
        // 验证用户权限:会话用户本人、按账户 ID 连接的会话、或账户属主,三者之一即可
        // (原先要求 user_id 必须等于会话 ?user_id=,按账户 ID 连接的会话用真实 user_id 撤单会被拒)
        // @yutiansut @quantaxis
        let session_owns = session_user_id == cancel_user_id
            || client_account_id.as_deref() == Some(session_user_id)
            || client_account_id
                .as_deref()
                .and_then(|a| self.account_mgr.get_account_owner(a))
                .as_deref()
                == Some(session_user_id);
        if !session_owns {
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
                    // ✨ 不再直发 `status: "CANCELLED"`:撤单后的订单视图(FINISHED + status_code 5
                    //    + volume_left)由路由按 DIFF 标准推入 trade.<account>.orders @yutiansut @quantaxis
                    log::info!("DIFF cancel order success: order_id={}", order_id);
                }
                Err(e) => {
                    // ✨ 撤单回执带单号与 CTP 错误码(hft_maker 据此决定是否重发)@yutiansut @quantaxis
                    //    25 撤单找不到相应报单 / 26 报单已全成交或已撤销,不能再撤 / 其余 91
                    let text = e.to_string();
                    let error_id = if text.contains("not found")
                        || text.contains("does not belong")
                    {
                        25
                    } else if text.contains("cannot be cancelled in status") {
                        26
                    } else {
                        91
                    };
                    let notify_patch = serde_json::json!({
                        "notify": {
                            "cancel_error": {
                                "type": "MESSAGE",
                                "level": "ERROR",
                                "code": 3002,
                                "order_id": order_id,
                                "error_id": error_id,
                                "content": format!(
                                    "{}: {}",
                                    crate::exchange::order_router::ctp_error_prompt(error_id),
                                    text
                                )
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
        conn_id: &str,
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

            self.snapshot_mgr.push_to_connection(conn_id, notify_patch);
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

            self.snapshot_mgr.push_to_connection(conn_id, notify_patch);
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
            let conn_id_str = conn_id.to_string();
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

                        // ⚠️ 模拟K线**默认关闭**,只有显式设 QAEX_MOCK_KLINES=1 才生成
                        //    @yutiansut @quantaxis
                        //
                        // 原来是「真实数据为空 ⇒ 静默返回随机 K 线」。这条路径在
                        // **生产 WS** 上,客户端完全无从分辨:数据来自服务端、看起来权威,
                        // 而且任何客户端(不只本前端)都会拿到假行情。
                        //
                        // 编造数据比崩溃更坏 —— 崩溃是可见的,编的数看起来完全正常。
                        // 同一类问题前端也有两处(结算页伪造最新价/涨跌幅、
                        // 因子页造 100 根假 K 线),均已改为「取真实值,取不到就留空」。
                        //
                        // 保留这个能力是因为它对图表渲染的联调确有价值,
                        // 但必须**显式开启**,且每次使用都打 WARN。
                        let klines = if klines.is_empty() {
                            let mock_on = std::env::var("QAEX_MOCK_KLINES")
                                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                                .unwrap_or(false);
                            if mock_on {
                                log::warn!(
                                    "⚠️ [DIFF set_chart] {} 无真实K线,QAEX_MOCK_KLINES=1 → 返回**模拟**数据(非真实行情)",
                                    instrument_id
                                );
                                generate_mock_klines(&instrument_id, period, count)
                            } else {
                                log::info!(
                                    "📊 [DIFF set_chart] {} 无真实K线,返回空列表(设 QAEX_MOCK_KLINES=1 可生成模拟数据)",
                                    instrument_id
                                );
                                Vec::new()
                            }
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
                            // @yutiansut @quantaxis: 使用字符串传输避免JavaScript精度丢失
                            let datetime_ns = kline.timestamp * 1_000_000; // 毫秒转纳秒

                            kline_data.insert(
                                kline_id.to_string(),
                                serde_json::json!({
                                    "datetime": datetime_ns.to_string(), // 字符串避免精度丢失
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

                        snapshot_mgr.push_to_connection(&conn_id_str, kline_patch);
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

                        snapshot_mgr.push_to_connection(&conn_id_str, error_patch);
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

            self.snapshot_mgr.push_to_connection(conn_id, notify_patch);
            log::error!("K-line service not available for set_chart request");
        }
    }

    /// DIFF `quotes.<inst>` 完整对象:10 档盘口 + 当日统计(`QuoteStats`)。@yutiansut @quantaxis
    ///
    /// 实时快照广播(`convert_market_event_to_diff`)与订阅时的初始快照
    /// (`handle_subscribe_quote`)共用这一份,两条路径字段保证一致。
    /// 字段沿用 DIFF/TqSdk 命名;`datetime` 维持现状为毫秒 epoch(同合约严格递增)。
    /// 缺档 / 当日尚无成交的字段写 `null` —— 按 JSON Merge Patch(RFC 7386)
    /// 这会**删除**客户端截面里的旧值,而不是留着一个过期的数。
    fn quote_value(
        instrument_id: &str,
        bids: &[crate::market::PriceLevel],
        asks: &[crate::market::PriceLevel],
        timestamp_ms: i64,
        stats: Option<&crate::market::QuoteStats>,
    ) -> serde_json::Value {
        use serde_json::json;
        let mut q = serde_json::Map::new();
        q.insert("instrument_id".into(), json!(instrument_id));
        q.insert("datetime".into(), json!(timestamp_ms));
        for i in 0..10 {
            q.insert(format!("bid_price{}", i + 1), json!(bids.get(i).map(|b| b.price)));
            q.insert(format!("bid_volume{}", i + 1), json!(bids.get(i).map(|b| b.volume)));
            q.insert(format!("ask_price{}", i + 1), json!(asks.get(i).map(|a| a.price)));
            q.insert(format!("ask_volume{}", i + 1), json!(asks.get(i).map(|a| a.volume)));
        }
        if let Some(st) = stats {
            q.insert("last_price".into(), json!(st.last_price));
            q.insert("volume".into(), json!(st.volume));
            q.insert("amount".into(), json!(st.amount));
            q.insert("open".into(), json!(st.open));
            q.insert("highest".into(), json!(st.highest));
            q.insert("lowest".into(), json!(st.lowest));
            q.insert("pre_close".into(), json!(st.pre_close));
            q.insert("pre_settlement".into(), json!(st.pre_settlement));
            q.insert("upper_limit".into(), json!(st.upper_limit));
            q.insert("lower_limit".into(), json!(st.lower_limit));
            q.insert("open_interest".into(), json!(st.open_interest));
            q.insert("volume_multiple".into(), json!(st.volume_multiple));
            q.insert("price_tick".into(), json!(st.price_tick));
        }
        serde_json::Value::Object(q)
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
                stats,
            } => Some(serde_json::json!({
                "quotes": {
                    instrument_id: Self::quote_value(instrument_id, bids, asks, *timestamp, stats.as_ref())
                }
            })),

            // ✨ 逐笔 Tick / LastPrice 不再单独推进 quotes @yutiansut @quantaxis
            //
            // quotes 由**实时**完整快照维护(每次盘口变化/成交后 ≤10ms 发一张,含
            // last_price 与当日累计 volume/amount),逐笔事件已体现在其中。原来每笔成交
            // 另推一个 `{last_price, volume, datetime}` 补丁,而 `volume` 写的是**这一笔**
            // 的量 —— 按 JSON Merge Patch 会把快照里的**累计**成交量覆盖成 1.0。
            // ⚠️ 这里**不是**降成定时快照:实时性由 snapshot_broadcaster 的事件驱动保证。
            MarketDataEvent::LastPrice { .. } | MarketDataEvent::Tick { .. } => None,

            MarketDataEvent::OrderBookUpdate {
                instrument_id: _,
                side: _,
                price: _,
                volume: _,
                timestamp: _,
            } => {
                // @yutiansut @quantaxis: 跳过单独的增量更新
                // 因为无法确定是哪一档价格变化，发送单档数据会覆盖5档盘口
                // 依赖 OrderBookSnapshot 定期发送完整5档数据（默认500ms间隔）
                None
            }

            MarketDataEvent::KLineFinished {
                instrument_id,
                period,
                kline,
                timestamp: _,
            } => {
                // 转换为 DIFF klines 格式（增量推送新K线）
                // @yutiansut @quantaxis
                let duration_ns = crate::market::kline::KLinePeriod::from_int(*period)
                    .map(|p| p.to_duration_ns())
                    .unwrap_or(0);

                // K线ID使用时间戳除以周期得到序列号
                let kline_id = (kline.timestamp * 1_000_000) / duration_ns; // 毫秒转纳秒后除以周期

                // DIFF协议要求datetime为UnixNano（纳秒）
                // @yutiansut @quantaxis: 使用字符串传输避免JavaScript精度丢失
                let datetime_ns = kline.timestamp * 1_000_000; // 毫秒转纳秒

                Some(serde_json::json!({
                    "klines": {
                        instrument_id: {
                            duration_ns.to_string(): {
                                "data": {
                                    kline_id.to_string(): {
                                        "datetime": datetime_ns.to_string(), // 字符串避免精度丢失
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
    async fn handle_peek_message(&self, conn_id: &str, ctx_addr: Addr<DiffWebsocketSession>) {
        let snapshot_mgr = self.snapshot_mgr.clone();
        let conn_id = conn_id.to_string();

        // 启动异步任务等待 peek(按连接:同一用户的多条连接各自 peek 各自的队列)
        tokio::spawn(async move {
            match snapshot_mgr.peek(&conn_id).await {
                Some(patches) => {
                    // 收到 patch，发送 rtn_data
                    let rtn_data = DiffServerMessage::RtnData { data: patches };

                    // 发送到 WebSocket session
                    ctx_addr.do_send(SendDiffMessage { message: rtn_data });
                }
                None => {
                    // ✨ 超时也必须回包 —— 原先只 log::debug 什么都不发。
                    //
                    // DIFF 是「客户端 peek → 服务端在有数据或超时时回 rtn_data →
                    // 客户端收到后再 peek」的长轮询环。服务端超时不回包,客户端就
                    // 永远收不到响应,也无从发下一个 peek —— 整条实时链路静默停摆:
                    // socket 仍是 CONNECTED、无报错、无重连,页面就是不再更新。
                    // 服务端 peek 超时是 30 秒(protocol/diff/snapshot.rs:153),
                    // 即只要 30 秒无任何 patch,这个连接的数据流就死了。
                    //
                    // 回一个空 data 的 rtn_data,把环续上。 @yutiansut @quantaxis
                    log::debug!("peek_message timeout for conn: {}, sending empty rtn_data", conn_id);
                    let rtn_data = DiffServerMessage::RtnData { data: vec![] };
                    ctx_addr.do_send(SendDiffMessage { message: rtn_data });
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
        let heartbeat_interval = std::env::var("QAEXCHANGE_WS_HEARTBEAT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(5));
        let client_timeout = std::env::var("QAEXCHANGE_WS_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(30));

        ctx.run_interval(heartbeat_interval, move |act, ctx| {
            if std::time::Instant::now().duration_since(act.heartbeat) > client_timeout {
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
        let n = crate::service::websocket::ws_connection_opened();
        log::debug!("WS 连接 +1 → {}", n);
        log::info!("DIFF WebSocket session {} started", self.session_id);
        // 登记本连接的截面,并绑定 `?user_id=` 主题(前端按账户 ID 连接、不发 req_login,
        // 靠这个绑定收回报)。在 started() 里登记:早于任何消息处理,也不会因握手失败而泄漏。
        // @yutiansut @quantaxis
        self.diff_handler.snapshot_mgr.register_connection(
            &self.session_id,
            self.user_id.as_deref().unwrap_or(""),
        );
        self.start_heartbeat(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        let n = crate::service::websocket::ws_connection_closed();
        log::debug!("WS 连接 -1 → {}", n);
        log::info!("DIFF WebSocket session {} stopped", self.session_id);

        // ✨ 从行情广播器注销(订阅者 ID = 连接 ID,见 handle_subscribe_quote)@yutiansut @quantaxis
        if let Some(ref broadcaster) = self.diff_handler.market_broadcaster {
            broadcaster.unsubscribe(&self.session_id);
        }

        // 注销本连接的截面(只删这一条连接;同一用户的其他连接不受影响)
        //
        // ⚠️ 这里**不能** tokio::spawn。`stopped()` 是同步生命周期钩子,
        // System 关停或跨线程 drop 时当前线程没有 tokio reactor,
        // spawn 会 panic 掉整个 arbiter → HTTP 服务停止监听。
        // remove_connection 本身只是 DashMap 操作,直接调。@yutiansut @quantaxis
        self.diff_handler.snapshot_mgr.remove_connection(&self.session_id);
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
                            let conn_id = self.session_id.clone();
                            let ctx_addr = ctx.address();

                            // 异步处理 DIFF 消息
                            ctx.spawn(
                                async move {
                                    handler
                                        .handle_diff_message(&conn_id, &user_id, diff_msg, ctx_addr)
                                        .await;
                                }
                                .into_actor(self),
                            );
                        } else {
                            // 未认证，只允许 ReqLogin 消息
                            if matches!(diff_msg, DiffClientMessage::ReqLogin { .. }) {
                                let handler = self.diff_handler.clone();
                                let user_id = "anonymous".to_string(); // 临时用户ID
                                let conn_id = self.session_id.clone();
                                let ctx_addr = ctx.address();

                                ctx.spawn(
                                    async move {
                                        handler
                                            .handle_diff_message(&conn_id, &user_id, diff_msg, ctx_addr)
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

    /// ✨ quotes 为完整快照(实时发布);逐笔不再单独推 quote 补丁(2026-11-08)@yutiansut @quantaxis
    #[test]
    fn quotes_are_full_snapshots_and_ticks_are_dropped() {
        use crate::market::{MarketDataEvent, PriceLevel, QuoteStats};
        let ev = MarketDataEvent::OrderBookSnapshot {
            instrument_id: "IH2501".into(),
            bids: vec![PriceLevel { price: 1809.8, volume: 2 }],
            asks: vec![],
            timestamp: 1_794_000_000_123,
            stats: Some(QuoteStats {
                last_price: 1810.0, volume: 6, amount: 3_258_000.0,
                open: Some(1810.0), highest: Some(1812.0), lowest: None,
                pre_close: 1810.0, pre_settlement: 1810.0, upper_limit: 1991.0, lower_limit: 1629.0,
                open_interest: 7, volume_multiple: 300.0, price_tick: 0.2,
            }),
        };
        let p = DiffHandler::convert_market_event_to_diff(&ev).expect("snapshot -> quote patch");
        let q = &p["quotes"]["IH2501"];
        assert_eq!(q["datetime"], 1_794_000_000_123_i64);
        assert_eq!(q["bid_price1"], 1809.8);
        assert_eq!(q["bid_volume1"], 2);
        assert!(q["ask_price1"].is_null(), "空的一侧写 null(Merge Patch 删除旧值)");
        assert!(q["bid_price10"].is_null());
        for k in ["last_price", "volume", "amount", "open", "highest", "pre_close",
                  "pre_settlement", "upper_limit", "lower_limit", "open_interest",
                  "volume_multiple", "price_tick"] {
            assert!(!q[k].is_null(), "{} 缺失", k);
        }
        assert_eq!(q["volume"], 6);
        assert!(q["lowest"].is_null(), "当日无该值时为 null");

        let tick = MarketDataEvent::Tick {
            instrument_id: "IH2501".into(), price: 1810.0, volume: 1.0,
            direction: "buy".into(), timestamp: 1,
        };
        assert!(DiffHandler::convert_market_event_to_diff(&tick).is_none(), "逐笔已体现在实时快照里,不单独推");
        let lp = MarketDataEvent::LastPrice { instrument_id: "IH2501".into(), price: 1810.0, timestamp: 1 };
        assert!(DiffHandler::convert_market_event_to_diff(&lp).is_none());
    }
}

#[cfg(test)]
mod initial_snapshot_tests {
    /// 首帧(登录初始快照)与增量帧(成交/撤单推送)的**字段集必须一致**。
    /// @yutiansut @quantaxis
    ///
    /// 这是本类 bug 的判据:两处各自构造 patch,字段集一旦分叉,
    /// 客户端 DIFF 合并出来的对象就会「时有时无某个字段」——
    /// 而单看任何一处代码都看不出问题(见 lessons L153)。
    ///
    /// 两处分别在:
    ///   首帧   `diff_handler.rs` 的登录分支(本次补的)
    ///   增量帧 `trade_gateway.rs:1775` 的 `push_account_update`
    #[test]
    fn initial_and_incremental_account_fields_match() {
        use std::collections::BTreeSet;
        let src = std::fs::read_to_string("src/service/websocket/diff_handler.rs")
            .expect("读 diff_handler.rs");
        let gw = std::fs::read_to_string("src/exchange/trade_gateway.rs")
            .expect("读 trade_gateway.rs");

        // ⚠️ 窗口必须够大 —— 我第一版取 600 字符,只够到 `balance`(距标记 570),
        // 而字段一直延伸到 847,于是测试报「字段集分叉」,**其实是量具太短**。
        // 下面的自检就是为这个加的。@yutiansut @quantaxis
        fn fields(hay: &str, marker: &str) -> BTreeSet<String> {
            let i = hay.find(marker).unwrap_or_else(|| panic!("找不到标记 {}", marker));
            let seg = &hay[i..(i + 1500).min(hay.len())];
            ["balance", "available", "margin", "position_profit", "risk_ratio"]
                .iter()
                .filter(|f| seg.contains(&format!("\"{}\":", f)))
                .map(|f| f.to_string())
                .collect()
        }

        let initial = fields(&src, "let mut accounts_patch");
        let incremental = fields(&gw, "\"accounts\": {");

        // 自检:**已知正确**的增量帧必须找齐 5 个字段。找不齐说明窗口太小或标记错了,
        // 此时任何「分叉」结论都不可信 —— 这比「非空」强得多:
        // 我第一版只判非空,而它在只找到 1 个字段时照样通过。
        assert_eq!(
            incremental.len(), 5,
            "自检失败:增量帧应能找到全部 5 个字段,实得 {:?} —— 量具有问题,本轮结论作废",
            incremental
        );
        assert_eq!(
            initial, incremental,
            "首帧与增量帧的账户字段集分叉了。\
             客户端合并后会出现「某个字段时有时无」,而单看一处代码看不出来。\
             首帧 {:?} / 增量帧 {:?}",
            initial, incremental
        );
    }
}
