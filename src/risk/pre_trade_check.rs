//! 盘前风控检查
//!
//! 在订单提交到撮合引擎前进行风险检查，包括：
//! - 资金充足性检查
//! - 持仓限额检查
//! - 订单合法性检查
//! - 自成交防范

use crate::core::{Order, QA_Account};
use crate::exchange::AccountManager;
use crate::ExchangeError;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 风控检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskCheckResult {
    /// 通过
    Pass,
    /// 拒绝
    Reject { reason: String, code: RiskCheckCode },
}

/// 风控拒绝代码
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RiskCheckCode {
    /// 资金不足
    InsufficientFunds = 1001,
    /// 超过持仓限额
    ExceedPositionLimit = 1002,
    /// 订单金额过大
    ExceedOrderLimit = 1003,
    /// 风险度过高
    HighRiskRatio = 1004,
    /// 自成交风险
    SelfTradingRisk = 1005,
    /// 账户不存在
    AccountNotFound = 1006,
    /// 合约不存在
    InstrumentNotFound = 1007,
    /// 订单参数非法
    InvalidOrderParams = 1008,
}

/// 风控配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// 单账户单品种最大持仓比例 (0.0-1.0)
    pub max_position_ratio: f64,

    /// 单笔订单最大金额
    pub max_order_amount: f64,

    /// 风险度阈值 (触发警告)
    pub risk_ratio_warning: f64,

    /// 风险度阈值 (拒绝下单)
    pub risk_ratio_reject: f64,

    /// 是否启用自成交防范
    pub enable_self_trade_prevention: bool,

    /// 最小订单数量
    pub min_order_volume: f64,

    /// 最大订单数量
    pub max_order_volume: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_ratio: 0.5,        // 50% 单品种持仓
            max_order_amount: 10_000_000.0, // 1000万单笔限额
            risk_ratio_warning: 0.8,        // 80% 风险度警告
            risk_ratio_reject: 0.95,        // 95% 拒绝下单
            enable_self_trade_prevention: true,
            min_order_volume: 1.0,     // 最小1手
            max_order_volume: 10000.0, // 最大10000手
        }
    }
}

/// 订单风控检查请求
#[derive(Debug, Clone)]
pub struct OrderCheckRequest {
    pub account_id: String, // 交易系统只关心账户ID
    pub instrument_id: String,
    pub direction: String, // BUY/SELL
    pub offset: String,    // OPEN/CLOSE
    pub volume: f64,
    pub price: f64,         // 向后兼容
    pub limit_price: f64,   // 订单价格（用于自成交检查）
    pub price_type: String, // LIMIT/MARKET/ANY（用于自成交检查）

    // ── 合约参数 —— 由 order_router 从 instrument_registry 取好传入 ──
    //
    // ⚠️ 为什么必须带进来:本模块此前所有金额都用 `price * volume` 直算,
    // **漏了合约乘数**。IF2501 乘数 300、保证金率 12%,
    // 一手 3800 点真实占用 136,800,而旧口径买开算 3,800、卖开算 760。
    // 后果:check_funds 的闸门形同虚设(实测 NOISE_BUY/NOISE_SELL 的
    // `available` 被冻成 -211,730 / -190,348),
    // check_position_limit 的比例分子小 300 倍 → 50% 的持仓限额实际允许 150 倍。
    //
    // 注册表查询放在 order_router(它本来就为合约状态检查查过一次),
    // 这里只接收结果 —— 避免本模块再持有一份 registry 引用,
    // 也避免同一订单查两次。@yutiansut @quantaxis
    pub contract_multiplier: f64,
    pub margin_rate: f64,
    pub commission_rate: f64,
}

impl Default for OrderCheckRequest {
    /// ⚠️ 仅供测试/示例构造使用。合约参数退回「乘数 1 / 保证金率 20% / 手续费万三」
    /// 的旧口径 —— **生产路径必须由 order_router 从 instrument_registry 取真值传入**,
    /// 不要依赖这里的默认值。@yutiansut @quantaxis
    fn default() -> Self {
        Self {
            account_id: String::new(),
            instrument_id: String::new(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 0.0,
            price: 0.0,
            limit_price: 0.0,
            price_type: "LIMIT".to_string(),
            contract_multiplier: 1.0,
            margin_rate: 0.2,
            commission_rate: 0.0003,
        }
    }
}

impl OrderCheckRequest {
    /// 名义金额 = 价格 × 数量 × 合约乘数
    #[inline]
    pub fn notional(&self) -> f64 {
        self.price * self.volume * self.contract_multiplier
    }
}

/// 活动订单信息（用于自成交防范）
#[derive(Debug, Clone)]
struct ActiveOrderInfo {
    order_id: String,
    instrument_id: String,
    direction: String,  // BUY/SELL
    limit_price: f64,   // 订单价格
    price_type: String, // LIMIT/MARKET/ANY
}

/// 盘前风控检查器
pub struct PreTradeCheck {
    /// 账户管理器引用
    account_mgr: Arc<AccountManager>,

    /// 风控配置
    config: Arc<RwLock<RiskConfig>>,

    /// 活动订单追踪 (user_id -> Vec<ActiveOrderInfo>)
    active_orders: DashMap<String, Arc<RwLock<Vec<ActiveOrderInfo>>>>,
}

impl PreTradeCheck {
    pub fn new(account_mgr: Arc<AccountManager>) -> Self {
        Self {
            account_mgr,
            config: Arc::new(RwLock::new(RiskConfig::default())),
            active_orders: DashMap::new(),
        }
    }

    /// 创建带自定义配置的检查器
    pub fn with_config(account_mgr: Arc<AccountManager>, config: RiskConfig) -> Self {
        Self {
            account_mgr,
            config: Arc::new(RwLock::new(config)),
            active_orders: DashMap::new(),
        }
    }

    /// 执行完整风控检查
    pub fn check(&self, req: &OrderCheckRequest) -> Result<RiskCheckResult, ExchangeError> {
        // 1. 基础参数检查
        self.check_order_params(req)?;

        // 2. 账户存在性检查（交易系统只关心account_id）
        let account = self.account_mgr.get_account(&req.account_id)?;

        // 3. 资金充足性检查
        if let Some(reject) = self.check_funds(&account, req)? {
            return Ok(reject);
        }

        // 4. 持仓限额检查
        if let Some(reject) = self.check_position_limit(&account, req)? {
            return Ok(reject);
        }

        // 5. 风险度检查
        if let Some(reject) = self.check_risk_ratio(&account)? {
            return Ok(reject);
        }

        // 6. 自成交防范检查
        if self.config.read().enable_self_trade_prevention {
            if let Some(reject) = self.check_self_trading(req)? {
                return Ok(reject);
            }
        }

        Ok(RiskCheckResult::Pass)
    }

    /// 检查订单参数合法性
    fn check_order_params(&self, req: &OrderCheckRequest) -> Result<(), ExchangeError> {
        let config = self.config.read();

        // 检查数量范围
        if req.volume < config.min_order_volume {
            return Err(ExchangeError::RiskCheckFailed(format!(
                "Order volume {} below minimum {}",
                req.volume, config.min_order_volume
            )));
        }

        if req.volume > config.max_order_volume {
            return Err(ExchangeError::RiskCheckFailed(format!(
                "Order volume {} exceeds maximum {}",
                req.volume, config.max_order_volume
            )));
        }

        // 检查价格合法性 @yutiansut @quantaxis
        // 市价单（MARKET）允许 price=0，限价单（LIMIT）要求 price > 0
        // ⚠️ 价格/数量必须先查**有限性**,再比大小。@yutiansut @quantaxis
        //
        // 原写法只有 `req.price <= 0.0` —— 那是 **NaN 不安全**的:
        // `NaN <= 0.0` 恒为 **false**,于是 NaN 价格会**直接通过这道检查**。
        // 对照 `service/http/models.rs:124` 的资金校验,那里刻意写成
        // `!(amount > 0.0)` 正是为了同时挡住 NaN —— 这里没跟上。
        //
        // 现状下 JSON 层能挡住字符串 `"NaN"`(serde 反序列化失败),
        // 但**计算产生的 NaN/Inf 挡不住**(例如上游用 0/0 算出的价格)。
        // 下面 `:251` 的金额上限之所以能拦住 inf,是因为 `inf > limit` 恰好为 true
        // —— **那是巧合不是设计**,而 `NaN > limit` 为 false,会漏过去。
        if !req.price.is_finite() || !req.volume.is_finite() {
            return Err(ExchangeError::RiskCheckFailed(format!(
                "订单价格或数量不是有限数值: price={}, volume={}",
                req.price, req.volume
            )));
        }
        if req.price_type != "MARKET" && !(req.price > 0.0) {
            return Err(ExchangeError::RiskCheckFailed(
                "Invalid order price for LIMIT order".to_string(),
            ));
        }

        // 订单金额上限。@yutiansut @quantaxis
        //
        // ⚠️ 原来是「市价单**整个跳过**金额检查」,理由写的是"价格可能是 0"。
        // 理由本身成立(市价单下单时可能不带价),但**跳过整个检查过头了** ——
        // `order_router.rs:490` 会先用最新价解析市价单并填回 `req.price`,
        // 所以走到这里时**通常是有价的**,完全可以估算名义金额。
        //
        // 后果(回验代理发现):`orders/index.vue` 把"市价单"连同表单里
        // 写死的 `price: 3800` 一起提交 —— 后端因 `price > 0` 跳过市价解析,
        // 又因 `price_type == "MARKET"` 跳过金额上限,
        // ⇒ **一张市价单可以任意大而不受风控闸门约束**。
        //
        // 改为:**有可用价格就查上限,只有真的拿不到价才跳过**。
        let order_amount = req.notional();   // 名义金额,含合约乘数
        if req.price_type == "MARKET" && !(order_amount > 0.0) {
            // 市价单且确实估不出金额(无价或价为 0)—— 此时无从判断,放行。
            // 注意用 `!(x > 0.0)` 而非 `x <= 0.0`:NaN 也要落进这一支
            // (上面已有有限性校验,这里是双保险)。
            return Ok(());
        }
        if order_amount > config.max_order_amount {
            return Err(ExchangeError::RiskCheckFailed(format!(
                "Order amount {} exceeds limit {}",
                order_amount, config.max_order_amount
            )));
        }

        Ok(())
    }

    /// 检查资金充足性
    fn check_funds(
        &self,
        account: &Arc<RwLock<QA_Account>>,
        req: &OrderCheckRequest,
    ) -> Result<Option<RiskCheckResult>, ExchangeError> {
        let acc = account.read();

        // 计算所需资金 —— 口径必须与 order_router 的预检一致(见 OrderCheckRequest 注释)
        let notional = req.notional();
        let estimated_commission = notional * req.commission_rate;
        let required_funds = if req.offset == "OPEN" {
            // 期货开仓买卖同向占用保证金
            notional * req.margin_rate + estimated_commission
        } else {
            // 平仓不占新保证金,只需手续费
            estimated_commission
        };

        if acc.money < required_funds {
            return Ok(Some(RiskCheckResult::Reject {
                reason: format!(
                    "Insufficient funds: available={:.2}, required={:.2}",
                    acc.money, required_funds
                ),
                code: RiskCheckCode::InsufficientFunds,
            }));
        }

        Ok(None)
    }

    /// 检查持仓限额
    fn check_position_limit(
        &self,
        account: &Arc<RwLock<QA_Account>>,
        req: &OrderCheckRequest,
    ) -> Result<Option<RiskCheckResult>, ExchangeError> {
        let acc = account.read();
        let config = self.config.read();

        // 如果是开仓，检查持仓限额 @yutiansut @quantaxis
        if req.offset == "OPEN" {
            let current_position = acc
                .hold
                .get(&req.instrument_id)
                .map(|pos| {
                    pos.volume_long_today
                        + pos.volume_long_his
                        + pos.volume_short_today
                        + pos.volume_short_his
                })
                .unwrap_or(0.0);

            let new_position = current_position + req.volume;
            // 使用可用资金作为总价值参考，避免除零
            // 对于新账户，使用 money（可用资金）而非 balance（可能为0）
            let total_value = if acc.accounts.balance > 0.0 {
                acc.accounts.balance
            } else if acc.money > 0.0 {
                acc.money
            } else {
                // 账户无资金时，拒绝开仓
                return Ok(Some(RiskCheckResult::Reject {
                    reason: "Insufficient funds: account balance is zero, please deposit first".to_string(),
                    code: RiskCheckCode::InsufficientFunds,
                }));
            };

            // 分子必须是**名义金额**(含乘数),否则比例小两个数量级 ——
            // 50% 的限额实际允许 150 倍持仓。@yutiansut @quantaxis
            let position_ratio =
                (new_position * req.price * req.contract_multiplier) / total_value;

            if position_ratio > config.max_position_ratio {
                return Ok(Some(RiskCheckResult::Reject {
                    reason: format!(
                        "Position ratio {:.2}% exceeds limit {:.2}%",
                        position_ratio * 100.0,
                        config.max_position_ratio * 100.0
                    ),
                    code: RiskCheckCode::ExceedPositionLimit,
                }));
            }
        }

        Ok(None)
    }

    /// 检查风险度
    fn check_risk_ratio(
        &self,
        account: &Arc<RwLock<QA_Account>>,
    ) -> Result<Option<RiskCheckResult>, ExchangeError> {
        let acc = account.read();
        let config = self.config.read();

        let risk_ratio = acc.accounts.risk_ratio;

        // 拒绝阈值
        if risk_ratio >= config.risk_ratio_reject {
            return Ok(Some(RiskCheckResult::Reject {
                reason: format!(
                    "Risk ratio {:.2}% exceeds reject threshold {:.2}%",
                    risk_ratio * 100.0,
                    config.risk_ratio_reject * 100.0
                ),
                code: RiskCheckCode::HighRiskRatio,
            }));
        }

        // 警告阈值 (不拒绝，但记录日志)
        if risk_ratio >= config.risk_ratio_warning {
            log::warn!(
                "High risk ratio for user {}: {:.2}%",
                acc.account_cookie,
                risk_ratio * 100.0
            );
        }

        Ok(None)
    }

    /// 自成交防范检查
    ///
    /// 注意：平仓单跳过自成交检查 @yutiansut @quantaxis
    /// - 平仓（CLOSE）和开仓（OPEN）是不同的业务场景
    /// - 自成交防范主要针对：同一账户的 OPEN 订单可能自己撮合
    /// - 例如：BUY OPEN @ 3800 和 SELL OPEN @ 3800 会自成交
    /// - 但是：BUY OPEN @ 3800 和 SELL CLOSE @ 3800 是正常平仓，应允许
    fn check_self_trading(
        &self,
        req: &OrderCheckRequest,
    ) -> Result<Option<RiskCheckResult>, ExchangeError> {
        // ✅ 平仓单跳过自成交检查 - 平仓和开仓是不同业务场景
        if req.offset == "CLOSE" || req.offset == "CLOSETODAY" || req.offset == "CLOSEYESTERDAY" {
            log::debug!(
                "Self-trading check skipped for CLOSE order: account={}, instrument={}, direction={}, offset={}",
                req.account_id, req.instrument_id, req.direction, req.offset
            );
            return Ok(None);
        }

        // 检查同一账户在同一合约上是否有对手方向的活动订单
        if let Some(orders_arc) = self.active_orders.get(&req.account_id) {
            let orders = orders_arc.read();

            // 确定对手方向
            let opposite_direction = if req.direction == "BUY" {
                "SELL"
            } else {
                "BUY"
            };

            // 检查是否存在同合约的对手方向订单，且价格会导致自成交
            for active_order in orders.iter() {
                if active_order.instrument_id == req.instrument_id
                    && active_order.direction == opposite_direction
                {
                    // ✅ 价格检查：只有当价格会导致成交时才拒绝
                    let would_match = match (req.direction.as_str(), req.price_type.as_str()) {
                        // 新订单是 BUY
                        ("BUY", "MARKET") => true, // 市价单总是可能成交
                        ("BUY", _) => {
                            // 限价买单：只有当买价 >= 已有卖单价格时才会成交
                            req.limit_price >= active_order.limit_price
                        }
                        // 新订单是 SELL
                        ("SELL", "MARKET") => true, // 市价单总是可能成交
                        ("SELL", _) => {
                            // 限价卖单：只有当卖价 <= 已有买单价格时才会成交
                            req.limit_price <= active_order.limit_price
                        }
                        _ => false,
                    };

                    if would_match {
                        log::warn!(
                            "Self-trading prevented: account={}, instrument={}, new_order={} @ {}, existing_order={} ({}) @ {}",
                            req.account_id,
                            req.instrument_id,
                            req.direction,
                            req.limit_price,
                            active_order.order_id,
                            active_order.direction,
                            active_order.limit_price
                        );

                        return Ok(Some(RiskCheckResult::Reject {
                            reason: format!(
                                "Self-trading prevented: existing {} order {} @ {} would match new {} @ {}",
                                active_order.direction,
                                active_order.order_id,
                                active_order.limit_price,
                                req.direction,
                                req.limit_price
                            ),
                            code: RiskCheckCode::SelfTradingRisk,
                        }));
                    } else {
                        log::debug!(
                            "Same account opposite orders allowed: {} @ {} vs existing {} @ {} (no match)",
                            req.direction,
                            req.limit_price,
                            active_order.direction,
                            active_order.limit_price
                        );
                    }
                }
            }
        }

        Ok(None)
    }

    /// 记录活动订单
    pub fn register_active_order(
        &self,
        account_id: &str,
        order_id: String,
        instrument_id: String,
        direction: String,
        limit_price: f64,
        price_type: String,
    ) {
        let order_info = ActiveOrderInfo {
            order_id,
            instrument_id,
            direction,
            limit_price,
            price_type,
        };

        self.active_orders
            .entry(account_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(Vec::new())))
            .write()
            .push(order_info);
    }

    /// 移除活动订单
    pub fn remove_active_order(&self, account_id: &str, order_id: &str) {
        if let Some(orders) = self.active_orders.get(account_id) {
            orders
                .write()
                .retain(|order_info| order_info.order_id != order_id);
        }
    }

    /// 获取账户活动订单数量
    pub fn get_active_order_count(&self, account_id: &str) -> usize {
        self.active_orders
            .get(account_id)
            .map(|orders| orders.read().len())
            .unwrap_or(0)
    }

    /// 更新风控配置
    pub fn update_config(&self, config: RiskConfig) {
        *self.config.write() = config;
    }

    /// 获取当前配置
    pub fn get_config(&self) -> RiskConfig {
        self.config.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::account_ext::{AccountType, OpenAccountRequest};

    fn create_test_account_manager() -> Arc<AccountManager> {
        let mgr = Arc::new(AccountManager::new());

        // 创建测试账户（使用固定ID）
        let req = OpenAccountRequest {
            user_id: "test_user".to_string(),
            account_id: Some("test_user".to_string()), // 使用固定ID
            account_name: "Test User".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        let account_id = mgr.open_account(req).unwrap();
        assert_eq!(account_id, "test_user"); // 验证账户ID
        mgr
    }

    #[test]
    fn test_check_order_params() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        // 正常订单
        let req = OrderCheckRequest {
            account_id: "test_account".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        assert!(checker.check_order_params(&req).is_ok());

        // 数量过小
        let req_small = OrderCheckRequest {
            volume: 0.5,
            ..req.clone()
        };
        assert!(checker.check_order_params(&req_small).is_err());

        // 价格非法
        let req_invalid_price = OrderCheckRequest {
            price: -10.0,
            ..req.clone()
        };
        assert!(checker.check_order_params(&req_invalid_price).is_err());
    }

    #[test]
    fn test_check_funds() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr.clone());

        let account = account_mgr.get_default_account("test_user").unwrap();

        // 资金充足
        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check_funds(&account, &req).unwrap();
        assert!(result.is_none()); // 通过检查

        // 资金不足
        let req_large = OrderCheckRequest {
            volume: 10000.0,
            price: 1000.0,
            ..req
        };

        let result = checker.check_funds(&account, &req_large).unwrap();
        assert!(result.is_some()); // 拒绝
        if let Some(RiskCheckResult::Reject { code, .. }) = result {
            assert_eq!(code, RiskCheckCode::InsufficientFunds);
        }
    }

    #[test]
    fn test_full_check() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check(&req).unwrap();
        assert!(matches!(result, RiskCheckResult::Pass));
    }

    #[test]
    fn test_active_order_tracking() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        assert_eq!(checker.get_active_order_count("test_user"), 0);

        checker.register_active_order(
            "test_user",
            "order1".to_string(),
            "IX2301".to_string(),
            "BUY".to_string(),
            100.0,
            "LIMIT".to_string(),
        );
        checker.register_active_order(
            "test_user",
            "order2".to_string(),
            "IX2302".to_string(),
            "SELL".to_string(),
            200.0,
            "LIMIT".to_string(),
        );
        assert_eq!(checker.get_active_order_count("test_user"), 2);

        checker.remove_active_order("test_user", "order1");
        assert_eq!(checker.get_active_order_count("test_user"), 1);
    }

    /// 非有限价格/数量必须被拒 —— 覆盖 NaN 与 ±Inf。@yutiansut @quantaxis
    ///
    /// 为什么要专门测:原实现只有 `req.price <= 0.0`,而 **`NaN <= 0.0` 恒为 false**,
    /// NaN 价格会直接通过。JSON 层能挡住字符串 "NaN",但**计算产生的** NaN 挡不住。
    /// 下游 `:251` 的金额上限拦得住 inf 纯属巧合(`inf > limit` 为 true),
    /// 对 NaN 无效(`NaN > limit` 为 false)。
    /// 市价单**带价时也要受金额上限约束**。@yutiansut @quantaxis
    ///
    /// 原实现是 `if price_type == "MARKET" { return Ok(()) }` —— 整个跳过。
    /// 回验代理发现:前端 `orders/index.vue` 会把"市价单"连同写死的
    /// `price: 3800` 一起提交,后端因 `price > 0` 跳过市价解析、
    /// 又因 MARKET 跳过金额上限 ⇒ 市价单可以任意大而不受风控约束。
    #[test]
    fn test_market_order_amount_limit_enforced_when_priced() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let mk = |price: f64, volume: f64, pt: &str| OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume,
            price,
            limit_price: price,
            price_type: pt.to_string(),
            contract_multiplier: 300.0,
            ..Default::default()
        };

        // ① 带价的市价单,金额远超上限 → **必须被拒**(改前会放行)
        let huge = checker.check(&mk(3800.0, 10000.0, "MARKET"));
        assert!(
            matches!(huge, Err(ExchangeError::RiskCheckFailed(_))),
            "带价市价单超上限应被拒,实际 {:?}",
            huge
        );

        // ② 真正无价的市价单 → 估不出金额,放行(保留原语义)
        let no_price = checker.check(&mk(0.0, 1.0, "MARKET"));
        assert!(
            !matches!(no_price, Err(ExchangeError::RiskCheckFailed(ref m)) if m.contains("Order amount")),
            "无价市价单不应因金额被拒,实际 {:?}",
            no_price
        );

        // ③ 阴性对照:正常大小的市价单必须通过 —— 否则上面的"拒"没有意义
        let ok = checker.check(&mk(100.0, 1.0, "MARKET"));
        assert!(
            matches!(ok, Ok(RiskCheckResult::Pass)),
            "正常市价单不应被拒,实际 {:?}",
            ok
        );
    }

    #[test]
    fn test_non_finite_price_volume_rejected() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let mk = |price: f64, volume: f64| OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume,
            price,
            limit_price: price,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        for (name, price, volume) in [
            ("NaN 价格", f64::NAN, 1.0),
            ("+Inf 价格", f64::INFINITY, 1.0),
            ("-Inf 价格", f64::NEG_INFINITY, 1.0),
            ("NaN 数量", 100.0, f64::NAN),
            ("+Inf 数量", 100.0, f64::INFINITY),
        ] {
            // ⚠️ 基础参数校验返回的是 `Err(RiskCheckFailed)`,**不是** `Ok(Reject)` ——
            // 这是本文件的既有约定:参数错误(:223/:230/:250/:256/:267)走 Err,
            // 业务规则拒单(:296/:339/:351,如自成交、持仓限额)走 Ok(Reject)。
            // 上层 order_router.rs:625/635 两种都接。
            let r = checker.check(&mk(price, volume));
            assert!(
                matches!(r, Err(ExchangeError::RiskCheckFailed(_))),
                "{} 应被拒绝,实际 {:?}",
                name,
                r
            );
        }

        // 阴性对照:正常值必须**通过**这一关(否则上面的"全拒"没有意义)
        let ok = checker.check(&mk(100.0, 1.0));
        assert!(
            matches!(ok, Ok(RiskCheckResult::Pass)),
            "正常价格/数量不应被拒,实际 {:?}",
            ok
        );
    }

    #[test]
    fn test_self_trading_prevention() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        // 注册一个 BUY 订单
        checker.register_active_order(
            "test_user",
            "order1".to_string(),
            "IX2301".to_string(),
            "BUY".to_string(),
            100.0,
            "LIMIT".to_string(),
        );

        // 尝试提交同合约的 SELL 订单（价格会导致成交，应被拒绝）
        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 100.0,
            limit_price: 100.0, // ✅ 卖价 100 <= 买价 100，会成交
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check(&req).unwrap();
        assert!(matches!(
            result,
            RiskCheckResult::Reject {
                code: RiskCheckCode::SelfTradingRisk,
                ..
            }
        ));

        // 同方向订单应该通过
        let req_same_direction = OrderCheckRequest {
            direction: "BUY".to_string(),
            ..req.clone()
        };

        let result = checker.check(&req_same_direction).unwrap();
        assert!(matches!(result, RiskCheckResult::Pass));

        // 不同合约应该通过
        let req_different_instrument = OrderCheckRequest {
            instrument_id: "IX2302".to_string(),
            ..req
        };

        let result = checker.check(&req_different_instrument).unwrap();
        assert!(matches!(result, RiskCheckResult::Pass));
    }

    // ==================== RiskCheckCode 枚举测试 @yutiansut @quantaxis ====================

    /// 测试 RiskCheckCode 枚举值
    /// 验证所有风控拒绝代码的数值正确
    #[test]
    fn test_risk_check_code_values() {
        assert_eq!(RiskCheckCode::InsufficientFunds as i32, 1001);
        assert_eq!(RiskCheckCode::ExceedPositionLimit as i32, 1002);
        assert_eq!(RiskCheckCode::ExceedOrderLimit as i32, 1003);
        assert_eq!(RiskCheckCode::HighRiskRatio as i32, 1004);
        assert_eq!(RiskCheckCode::SelfTradingRisk as i32, 1005);
        assert_eq!(RiskCheckCode::AccountNotFound as i32, 1006);
        assert_eq!(RiskCheckCode::InstrumentNotFound as i32, 1007);
        assert_eq!(RiskCheckCode::InvalidOrderParams as i32, 1008);
    }

    /// 测试 RiskCheckCode 的相等性比较
    #[test]
    fn test_risk_check_code_equality() {
        let code1 = RiskCheckCode::InsufficientFunds;
        let code2 = RiskCheckCode::InsufficientFunds;
        let code3 = RiskCheckCode::HighRiskRatio;

        assert_eq!(code1, code2);
        assert_ne!(code1, code3);
    }

    // ==================== RiskConfig 配置测试 @yutiansut @quantaxis ====================

    /// 测试 RiskConfig 默认值
    /// 验证默认风控配置符合预期
    #[test]
    fn test_risk_config_default() {
        let config = RiskConfig::default();

        assert_eq!(config.max_position_ratio, 0.5);
        assert_eq!(config.max_order_amount, 10_000_000.0);
        assert_eq!(config.risk_ratio_warning, 0.8);
        assert_eq!(config.risk_ratio_reject, 0.95);
        assert!(config.enable_self_trade_prevention);
        assert_eq!(config.min_order_volume, 1.0);
        assert_eq!(config.max_order_volume, 10000.0);
    }

    /// 测试自定义 RiskConfig
    #[test]
    fn test_risk_config_custom() {
        let config = RiskConfig {
            max_position_ratio: 0.3,
            max_order_amount: 5_000_000.0,
            risk_ratio_warning: 0.7,
            risk_ratio_reject: 0.9,
            enable_self_trade_prevention: false,
            min_order_volume: 5.0,
            max_order_volume: 5000.0,
        };

        assert_eq!(config.max_position_ratio, 0.3);
        assert_eq!(config.max_order_amount, 5_000_000.0);
        assert!(!config.enable_self_trade_prevention);
    }

    /// 测试 RiskConfig 的 Clone trait
    #[test]
    fn test_risk_config_clone() {
        let config1 = RiskConfig::default();
        let config2 = config1.clone();

        assert_eq!(config1.max_position_ratio, config2.max_position_ratio);
        assert_eq!(config1.max_order_amount, config2.max_order_amount);
    }

    // ==================== PreTradeCheck 创建测试 @yutiansut @quantaxis ====================

    /// 测试使用自定义配置创建 PreTradeCheck
    #[test]
    fn test_pre_trade_check_with_config() {
        let account_mgr = create_test_account_manager();
        let config = RiskConfig {
            max_position_ratio: 0.3,
            ..RiskConfig::default()
        };

        let checker = PreTradeCheck::with_config(account_mgr, config);

        let loaded_config = checker.get_config();
        assert_eq!(loaded_config.max_position_ratio, 0.3);
    }

    /// 测试更新风控配置
    #[test]
    fn test_update_config() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        // 初始配置
        let initial_config = checker.get_config();
        assert_eq!(initial_config.max_position_ratio, 0.5);

        // 更新配置
        let new_config = RiskConfig {
            max_position_ratio: 0.2,
            ..RiskConfig::default()
        };
        checker.update_config(new_config);

        // 验证更新
        let updated_config = checker.get_config();
        assert_eq!(updated_config.max_position_ratio, 0.2);
    }

    // ==================== 订单参数检查测试 @yutiansut @quantaxis ====================

    /// 测试订单数量超过最大值
    #[test]
    fn test_check_order_params_volume_too_large() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let req = OrderCheckRequest {
            account_id: "test_account".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 20000.0, // 超过默认最大值 10000
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check_order_params(&req);
        assert!(result.is_err());
    }

    /// 测试市价单允许价格为0
    /// 市价单（MARKET）允许 price=0，限价单（LIMIT）要求 price > 0
    #[test]
    fn test_check_order_params_market_order_zero_price() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let req = OrderCheckRequest {
            account_id: "test_account".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 0.0, // 市价单允许价格为0
            limit_price: 0.0,
            price_type: "MARKET".to_string(),
            ..Default::default()
        };

        assert!(checker.check_order_params(&req).is_ok());
    }

    /// 测试订单金额超限
    /// max_order_amount 默认为 1000万
    #[test]
    fn test_check_order_params_amount_exceeds_limit() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let req = OrderCheckRequest {
            account_id: "test_account".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 100000.0, // 100000 * 200 = 2000万，超过限额
            price: 200.0,
            limit_price: 200.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check_order_params(&req);
        assert!(result.is_err());
    }

    // ==================== 资金检查测试 @yutiansut @quantaxis ====================

    /// 测试卖出开仓的资金检查
    /// 卖出开仓需要保证金 (20%)
    #[test]
    fn test_check_funds_sell_open() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr.clone());

        let account = account_mgr.get_default_account("test_user").unwrap();

        // 卖出开仓，100000 * 100 * 0.2 = 2000000 保证金，超过账户资金
        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "OPEN".to_string(),
            volume: 100000.0,
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check_funds(&account, &req).unwrap();
        assert!(result.is_some());
        if let Some(RiskCheckResult::Reject { code, .. }) = result {
            assert_eq!(code, RiskCheckCode::InsufficientFunds);
        }
    }

    /// 测试平仓只需手续费
    /// 平仓操作只需要手续费，不需要额外资金
    #[test]
    fn test_check_funds_close_position() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr.clone());

        let account = account_mgr.get_default_account("test_user").unwrap();

        // 平仓只需手续费
        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "CLOSE".to_string(),
            volume: 10.0,
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check_funds(&account, &req).unwrap();
        assert!(result.is_none()); // 应该通过
    }

    // ==================== 自成交防范测试 @yutiansut @quantaxis ====================

    /// 测试平仓订单跳过自成交检查
    /// CLOSE 订单不应触发自成交检查
    #[test]
    fn test_self_trading_close_order_allowed() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        // 注册一个 BUY OPEN 订单
        checker.register_active_order(
            "test_user",
            "order1".to_string(),
            "IX2301".to_string(),
            "BUY".to_string(),
            100.0,
            "LIMIT".to_string(),
        );

        // 提交 SELL CLOSE 订单（应该被允许，不是自成交）
        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "CLOSE".to_string(), // 平仓
            volume: 10.0,
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check_self_trading(&req).unwrap();
        assert!(result.is_none()); // 平仓订单跳过自成交检查
    }

    /// 测试价格不重叠的订单不构成自成交
    /// 买单 @ 100，卖单 @ 120 不会成交
    #[test]
    fn test_self_trading_price_no_overlap() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        // 注册一个 BUY @ 100 订单
        checker.register_active_order(
            "test_user",
            "order1".to_string(),
            "IX2301".to_string(),
            "BUY".to_string(),
            100.0, // 买价 100
            "LIMIT".to_string(),
        );

        // 提交 SELL @ 120 订单（卖价 > 买价，不会成交）
        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 120.0,
            limit_price: 120.0, // 卖价 120 > 买价 100，不会成交
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check(&req).unwrap();
        assert!(matches!(result, RiskCheckResult::Pass));
    }

    /// 测试市价单总是可能自成交
    #[test]
    fn test_self_trading_market_order() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        // 注册一个 SELL OPEN 订单
        checker.register_active_order(
            "test_user",
            "order1".to_string(),
            "IX2301".to_string(),
            "SELL".to_string(),
            100.0,
            "LIMIT".to_string(),
        );

        // 提交 BUY OPEN 市价单（市价单总是可能成交）
        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 0.0,
            limit_price: 0.0,
            price_type: "MARKET".to_string(),
            ..Default::default()
        };

        let result = checker.check_self_trading(&req).unwrap();
        assert!(result.is_some());
        if let Some(RiskCheckResult::Reject { code, .. }) = result {
            assert_eq!(code, RiskCheckCode::SelfTradingRisk);
        }
    }

    // ==================== 活动订单管理测试 @yutiansut @quantaxis ====================

    /// 测试移除不存在的订单
    #[test]
    fn test_remove_nonexistent_order() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        // 尝试移除不存在的订单（不应 panic）
        checker.remove_active_order("test_user", "nonexistent_order");
        assert_eq!(checker.get_active_order_count("test_user"), 0);
    }

    /// 测试多账户活动订单隔离
    #[test]
    fn test_active_orders_account_isolation() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        checker.register_active_order(
            "user1",
            "order1".to_string(),
            "IX2301".to_string(),
            "BUY".to_string(),
            100.0,
            "LIMIT".to_string(),
        );

        checker.register_active_order(
            "user2",
            "order2".to_string(),
            "IX2301".to_string(),
            "SELL".to_string(),
            100.0,
            "LIMIT".to_string(),
        );

        // 不同账户的订单应该隔离
        assert_eq!(checker.get_active_order_count("user1"), 1);
        assert_eq!(checker.get_active_order_count("user2"), 1);

        // 移除 user1 的订单不影响 user2
        checker.remove_active_order("user1", "order1");
        assert_eq!(checker.get_active_order_count("user1"), 0);
        assert_eq!(checker.get_active_order_count("user2"), 1);
    }

    // ==================== 完整检查流程测试 @yutiansut @quantaxis ====================

    /// 测试账户不存在的情况
    #[test]
    fn test_check_account_not_found() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let req = OrderCheckRequest {
            account_id: "nonexistent_account".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check(&req);
        assert!(result.is_err());
    }

    /// 测试禁用自成交防范
    #[test]
    fn test_self_trading_prevention_disabled() {
        let account_mgr = create_test_account_manager();
        let config = RiskConfig {
            enable_self_trade_prevention: false,
            ..RiskConfig::default()
        };
        let checker = PreTradeCheck::with_config(account_mgr, config);

        // 注册一个 BUY 订单
        checker.register_active_order(
            "test_user",
            "order1".to_string(),
            "IX2301".to_string(),
            "BUY".to_string(),
            100.0,
            "LIMIT".to_string(),
        );

        // 提交对手方向订单（应该通过，因为自成交防范已禁用）
        let req = OrderCheckRequest {
            account_id: "test_user".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "SELL".to_string(),
            offset: "OPEN".to_string(),
            volume: 10.0,
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        let result = checker.check(&req).unwrap();
        assert!(matches!(result, RiskCheckResult::Pass));
    }

    // ==================== 边界条件测试 @yutiansut @quantaxis ====================

    /// 测试订单数量刚好等于最小值
    #[test]
    fn test_volume_at_minimum() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let req = OrderCheckRequest {
            account_id: "test_account".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 1.0, // 刚好等于最小值
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        assert!(checker.check_order_params(&req).is_ok());
    }

    /// 测试订单数量刚好等于最大值
    #[test]
    fn test_volume_at_maximum() {
        let account_mgr = create_test_account_manager();
        let checker = PreTradeCheck::new(account_mgr);

        let req = OrderCheckRequest {
            account_id: "test_account".to_string(),
            instrument_id: "IX2301".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10000.0, // 刚好等于最大值
            price: 100.0,
            limit_price: 100.0,
            price_type: "LIMIT".to_string(),
            ..Default::default()
        };

        assert!(checker.check_order_params(&req).is_ok());
    }
}
