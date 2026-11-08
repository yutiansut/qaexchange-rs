//! 账户管理扩展功能
//!
//! 在 qars::QA_Account 基础上提供交易所特定的扩展功能

use crate::core::QA_Account;
use crate::ExchangeError;
use serde::{Deserialize, Serialize};

/// 开户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAccountRequest {
    /// 用户ID（用于绑定关系）
    pub user_id: String,

    /// 账户ID（可选，如果为空则自动生成）
    pub account_id: Option<String>,

    /// 账户名称
    pub account_name: String,

    /// 初始资金
    pub init_cash: f64,

    /// 账户类型
    pub account_type: AccountType,
}

/// 账户类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AccountType {
    /// 个人账户
    Individual,
    /// 机构账户
    Institutional,
    /// 做市商账户
    MarketMaker,
}

/// 入金请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRequest {
    pub account_id: String,
    pub amount: f64,
    pub reference: String, // 入金流水号
}

/// 出金请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawRequest {
    pub account_id: String,
    pub amount: f64,
    pub reference: String, // 出金流水号
}

/// 账户扩展 trait
pub trait AccountExt {
    /// 验证账户状态
    fn validate(&self) -> Result<(), ExchangeError>;

    /// 检查是否可以交易
    fn can_trade(&self) -> bool;

    /// 检查资金是否充足
    fn has_sufficient_funds(&self, required: f64) -> bool;

    /// **对外序列化专用**的风险度 —— 保证是有限数 @yutiansut @quantaxis
    ///
    /// qars 的 `get_riskratio()` 是 `margin / balance`,对
    /// `init_cash = 0` 的账户是 `0.0 / 0.0 = NaN`。而 serde_json 把非有限 f64
    /// **静默写成 `null`**(`value/ser.rs` → `Number::from_f64` 返回 None),
    /// 前端一个 `|| 0` 再把 null 兜成 0 —— 于是「风险度」显示成一个
    /// 看不出来的错值。实测线上 57 个账户里 19 个是这种情况。
    ///
    /// 语义:
    /// - `NaN`(0/0)  → `0.0`  —— 既无权益也无保证金,没有风险敞口
    /// - `+Inf`(x/0) → `999.0` —— 有保证金却零权益,用明确超阈的有限数表达
    /// - `-Inf`      → `0.0`
    ///
    /// ⚠️ 只用于**展示 / 序列化**。风险**判级**要用原始值,
    /// 因为 NaN 与 +Inf 的含义完全相反(见 `RiskLevel::from_risk_ratio`)。
    fn safe_risk_ratio(&mut self) -> f64;

    /// **QIFI 规范口径**的动态权益 @yutiansut @quantaxis
    ///
    /// qars 的 `get_balance()`(`account.rs:525`)是
    /// `static_balance + deposit - withdraw + float_profit + close_profit - commission`,
    /// 与 QIFI 规范有三处差异:
    ///   ① 它的 `static_balance` 语义是「初始资金」(构造时 = init_cash 后永不更新),
    ///      所以要自己再加一次 `deposit - withdraw`;而规范的
    ///      `static_balance = pre_balance + deposit - withdraw` 本身已含入出金。
    ///      **两套语义混用会把入金算两遍** —— 曾造成 ¥1,994,454,806 的虚增,
    ///      见 `account_mgr::conform_qifi_static_balance` 上的事故复盘。
    ///   ② 它用 `float_profit`(对开仓价),规范用 `position_profit`(对持仓价)。
    ///   ③ 它不含 `premium`(权利金)。
    ///
    /// 本函数按**规范**算,只依赖 `accounts` 上的规范字段 + 实时持仓盈亏,
    /// 与 qars 内部 `static_balance` 的语义无关,因此**不受污染影响**。
    /// 用它统一 HTTP 投影与 WebSocket 推送两条路径的口径。
    fn qifi_balance(&mut self) -> f64;
}

impl AccountExt for QA_Account {
    fn validate(&self) -> Result<(), ExchangeError> {
        if self.money < 0.0 {
            return Err(ExchangeError::AccountError(
                "Account balance cannot be negative".to_string(),
            ));
        }

        if self.accounts.risk_ratio > 1.0 {
            return Err(ExchangeError::RiskCheckFailed(format!(
                "Risk ratio too high: {}",
                self.accounts.risk_ratio
            )));
        }

        Ok(())
    }

    fn can_trade(&self) -> bool {
        self.accounts.available > 0.0 && self.accounts.risk_ratio < 1.0
    }

    fn has_sufficient_funds(&self, required: f64) -> bool {
        self.accounts.available >= required
    }

    /// ⚠️ **本函数的分子不含冻结保证金,而各端点显示的 `margin` 含** —— 口径冲突,
    /// 待决策,**不要在没拍板前单方面改**。@yutiansut @quantaxis
    ///
    /// qars `get_riskratio()`(`account.rs:314`)是 `get_margin() / accounts.balance`,
    /// 分子只有**持仓**保证金。而 `handlers.rs:282` 与 `management.rs:167` 都
    /// 刻意报 `margin = get_margin() + get_frozen_margin()`
    /// (management.rs:164 的注释:「保证金 = 持仓保证金 + 冻结保证金(待成交订单)」)。
    ///
    /// 后果是**同一份响应自己跟自己矛盾**。线上实测 `ACC_14805c37…`:
    /// ```text
    /// balance        2,132,919,328.77
    /// margin(报出)      32,066,810.46   = 持仓 20,002,766.06 + 冻结 12,064,044.40
    /// risk_ratio(报出)       0.009378   = 20,002,766.06 / balance   ← 只用持仓
    /// 拿页面上的数自己除      0.015034   = 32,066,810.46 / balance
    /// ```
    /// 用户按显示值验算得到 0.015034,页面却写 0.009378 —— **风险低估 1.60×**。
    ///
    /// 只在账户**有挂单**时才显形(冻结为 0 时两口径重合)。线上 4 个有保证金的账户
    /// 里 1 个分歧 —— 而那恰好是做市商的常态,也是风险最该被看见的时候。
    ///
    /// 为什么不直接改:`get_riskratio()` 还被 `risk_monitor.rs:423,712`
    /// (告警与强平判定)和 `settlement.rs:1059` 用着,把冻结算进去会**改变
    /// 告警与强平的触发线** —— 那是风控政策决定,不是实现细节。
    /// 两种改法各有代价,已记入 `tasks/DECISIONS-PENDING.md` 等拍板:
    ///   (A) 分子改成 持仓+冻结 —— 风险口径更保守,但触发线会下移;
    ///   (B) 端点改回只报持仓保证金(QIFI 规范里 `margin` 就是持仓保证金,
    ///       冻结另有 `frozen_margin` 字段)—— 不动风控,但会推翻
    ///       management.rs:164 那条「刻意合并」的决定。
    fn safe_risk_ratio(&mut self) -> f64 {
        let r = self.get_riskratio();
        if r.is_nan() {
            0.0
        } else if r.is_infinite() {
            if r > 0.0 { 999.0 } else { 0.0 }
        } else {
            r
        }
    }

    fn qifi_balance(&mut self) -> f64 {
        let position_profit = self.get_positionprofit();
        let a = &self.accounts;
        let static_balance = a.pre_balance + a.deposit - a.withdraw;
        let b = static_balance + a.close_profit + position_profit - a.commission + a.premium;
        if b == 0.0 { 0.0 } else { b }   // 归一负零
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_type_serialization() {
        let account_type = AccountType::Individual;
        let json = serde_json::to_string(&account_type).unwrap();
        let deserialized: AccountType = serde_json::from_str(&json).unwrap();
        assert_eq!(account_type, deserialized);
    }
}

// ════════════ 按合约覆盖 qars MarketPreset 的费率 ════════════ @yutiansut @quantaxis
//
// qars 的手续费与冻结保证金都取自**持仓对象自带的费率副本** `QA_Position.preset`
// (qars account.rs:成交 `calc_commission`、下单冻结 `calc_coeff` / `calc_sellopencoeff`),
// 副本在建仓时从全局 MarketPreset 复制。qars 是共享库、全局预设是 lazy_static 不可改 ——
// 覆盖只能落在副本上:下单前(必要时先建仓)、快照恢复后改写它;回报里的手续费用 `effective_preset`。
// 用例(2026-11-08):郑商所 FG 真实手续费开 / 平 / 平今均 2 元/手,qars 预设为开 3 / 平今 6
// (且 qars 成交时一律用 calc_commission,不区分平今);FG 保证金 9%、SA 8%,qars 预设 5%。

use dashmap::DashMap;
use qars::qaaccount::marketpreset::{CodePreset, MarketPreset};
use std::sync::OnceLock;

/// 单个合约的费率覆盖(缺省字段 = 沿用 qars 预设)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PresetOverride {
    /// 每手手续费(开 / 平 / 平今同价);设了就不再按成交额收
    pub commission_per_lot: Option<f64>,
    /// 按成交额的手续费率(开 / 平 / 平今同率);设了就不再按手收
    pub commission_rate: Option<f64>,
    /// 保证金率(多空同)
    pub margin_rate: Option<f64>,
}

impl PresetOverride {
    pub fn is_empty(&self) -> bool {
        self.commission_per_lot.is_none() && self.commission_rate.is_none() && self.margin_rate.is_none()
    }

    /// 改写一份费率副本。`with_margin = false` 时只改手续费(见 `apply_to_position`)。
    pub fn apply(&self, p: &mut CodePreset) {
        self.apply_parts(p, true)
    }

    fn apply_parts(&self, p: &mut CodePreset, with_margin: bool) {
        if let Some(x) = self.commission_per_lot {
            p.commission_coeff_pervol = x;
            p.commission_coeff_today_pervol = x;
            p.commission_coeff_peramount = 0.0;
            p.commission_coeff_today_peramount = 0.0;
        }
        if let Some(r) = self.commission_rate {
            p.commission_coeff_peramount = r;
            p.commission_coeff_today_peramount = r;
            p.commission_coeff_pervol = 0.0;
            p.commission_coeff_today_pervol = 0.0;
        }
        if with_margin {
            if let Some(m) = self.margin_rate {
                p.buy_frozen_coeff = m;
                p.sell_frozen_coeff = m;
            }
        }
    }

    /// 改写持仓的费率副本。⚠️ 保证金率**只在空仓时**改:qars 开仓按当时的系数把保证金加进
    /// `margin_long/short`,平仓按「当前系数 × 开仓均价」释放 —— 带着旧系数开的仓若中途换系数,
    /// 平仓会多放 / 少放保证金(例:5% 开、9% 平 ⇒ margin 变负)。手续费逐笔收取,随时可换。
    pub fn apply_to_position(&self, pos: &mut qars::qaaccount::position::QA_Position) {
        let flat = pos.volume_long_today + pos.volume_long_his + pos.volume_short_today + pos.volume_short_his
            + pos.volume_long_frozen_today + pos.volume_long_frozen_his
            + pos.volume_short_frozen_today + pos.volume_short_frozen_his
            == 0.0
            && pos.margin_long.abs() < 1e-6
            && pos.margin_short.abs() < 1e-6;
        self.apply_parts(&mut pos.preset, flat);
    }
}

fn preset_overrides() -> &'static DashMap<String, PresetOverride> {
    static M: OnceLock<DashMap<String, PresetOverride>> = OnceLock::new();
    M.get_or_init(DashMap::new)
}

/// 合约代码去掉交易所前缀(QIFI 持仓键形如 `CZCE.FG2701`)
fn bare_code(code: &str) -> &str {
    code.rsplit('.').next().unwrap_or(code)
}

/// 从合约配置登记覆盖(启动时调用,须早于账户恢复)。返回登记的合约数。
pub fn set_preset_overrides(configs: &[crate::utils::config::InstrumentConfig]) -> usize {
    let m = preset_overrides();
    let mut n = 0;
    for c in configs {
        let o = PresetOverride {
            commission_per_lot: c.commission_per_lot,
            commission_rate: c.commission_rate,
            margin_rate: c.margin_rate,
        };
        if !o.is_empty() {
            m.insert(c.instrument_id.clone(), o);
            n += 1;
        }
    }
    n
}

/// 直接登记一条覆盖(测试 / 管理接口用)
pub fn set_preset_override(instrument_id: &str, o: PresetOverride) {
    preset_overrides().insert(instrument_id.to_string(), o);
}

/// 生效费率 = qars 预设 + 合约覆盖
pub fn effective_preset(instrument_id: &str) -> CodePreset {
    let mut p = MarketPreset::global().get(instrument_id);
    if let Some(o) = preset_overrides().get(bare_code(instrument_id)) {
        o.apply(&mut p);
    }
    p
}

/// 改写账户里某合约持仓的费率副本。`create = true` 时持仓不存在就先建 ——
/// 下单前调用,让该合约第一张单的冻结保证金也按覆盖值算(qars `send_order` 内部建仓用的是预设)。
pub fn apply_preset_override(acc: &mut QA_Account, instrument_id: &str, create: bool) {
    let Some(o) = preset_overrides().get(instrument_id).map(|e| e.value().clone()) else {
        return;
    };
    if create && !acc.hold.contains_key(instrument_id) {
        acc.init_h(instrument_id);
    }
    if let Some(pos) = acc.hold.get_mut(instrument_id) {
        o.apply_to_position(pos);
    }
}

/// 改写账户全部持仓的费率副本(快照恢复后调用)。按持仓自身的合约代码匹配,不依赖哈希键格式。
pub fn apply_preset_overrides_all(acc: &mut QA_Account) -> usize {
    let m = preset_overrides();
    if m.is_empty() {
        return 0;
    }
    let mut n = 0;
    for (key, pos) in acc.hold.iter_mut() {
        let hit = [key.as_str(), pos.code.as_str(), pos.instrument_id.as_str()]
            .iter()
            .find_map(|c| m.get(bare_code(c)).map(|e| e.value().clone()));
        if let Some(o) = hit {
            o.apply_to_position(pos);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod preset_override_tests {
    use super::*;

    // 覆盖表是全局的:每个测试用自己的合约代码,互不干扰(仍以 FG 开头,qars 能取到 FG 预设)
    #[test]
    fn per_lot_commission_and_margin_override() {
        set_preset_override(
            "FG2791",
            PresetOverride { commission_per_lot: Some(2.0), commission_rate: None, margin_rate: Some(0.09) },
        );
        let p = effective_preset("FG2791");
        assert_eq!(p.calc_commission(882.0, 3.0), 6.0, "3 手 × 2 元");
        assert!((p.calc_coeff() - 0.09 * 20.0).abs() < 1e-12, "保证金 9% × 乘数 20");
        // 交易所前缀也认
        assert_eq!(effective_preset("CZCE.FG2791").calc_commission(882.0, 1.0), 2.0);
        // 未覆盖的合约沿用 qars 预设(FG 开 3 元/手)
        assert_eq!(effective_preset("FG2792").calc_commission(882.0, 1.0), 3.0);
    }

    #[test]
    fn account_positions_get_patched_before_first_order_and_after_restore() {
        set_preset_override(
            "FG2793",
            PresetOverride { commission_per_lot: Some(2.0), commission_rate: None, margin_rate: None },
        );
        let mut acc = QA_Account::new("acc_ovr", "default", "acc_ovr", 1_000_000.0, false, "sim");
        // 下单前:持仓不存在 → 先建再改写
        apply_preset_override(&mut acc, "FG2793", true);
        assert_eq!(acc.hold["FG2793"].preset.commission_coeff_pervol, 2.0);
        assert_eq!(acc.hold["FG2793"].preset.commission_coeff_today_pervol, 2.0);
        // 恢复后:把副本改回预设,再整体改写
        acc.hold.get_mut("FG2793").unwrap().preset = MarketPreset::global().get("FG2793");
        assert_eq!(acc.hold["FG2793"].preset.commission_coeff_pervol, 3.0);
        assert_eq!(apply_preset_overrides_all(&mut acc), 1);
        assert_eq!(acc.hold["FG2793"].preset.commission_coeff_pervol, 2.0);
        // 没有覆盖的合约不动、也不建仓
        apply_preset_override(&mut acc, "IH2501", true);
        assert!(!acc.hold.contains_key("IH2501"));
    }

    /// 带着旧系数开的仓:只换手续费,保证金系数等平仓后再换(否则平仓释放与开仓占用不对称)
    #[test]
    fn open_position_keeps_margin_coeff_until_flat() {
        set_preset_override(
            "FG2794",
            PresetOverride { commission_per_lot: Some(2.0), commission_rate: None, margin_rate: Some(0.09) },
        );
        let mut acc = QA_Account::new("acc_ovr2", "default", "acc_ovr2", 1_000_000.0, false, "sim");
        acc.init_h("FG2794");
        {
            let pos = acc.hold.get_mut("FG2794").unwrap();
            pos.volume_long_his = 3.0;
            pos.margin_long = 900.0 * 20.0 * 3.0 * 0.05; // 按旧系数 5% 占用
        }
        apply_preset_overrides_all(&mut acc);
        let p = &acc.hold["FG2794"].preset;
        assert_eq!(p.commission_coeff_pervol, 2.0, "手续费立即生效");
        assert_eq!(p.buy_frozen_coeff, 0.05, "有持仓时保证金系数不换");
        {
            let pos = acc.hold.get_mut("FG2794").unwrap();
            pos.volume_long_his = 0.0;
            pos.margin_long = 0.0;
        }
        apply_preset_override(&mut acc, "FG2794", true);
        assert_eq!(acc.hold["FG2794"].preset.buy_frozen_coeff, 0.09, "空仓后换成新系数");
    }

    #[test]
    fn rate_override_replaces_per_lot() {
        let mut p = MarketPreset::global().get("FG2701"); // 预设按手
        PresetOverride { commission_per_lot: None, commission_rate: Some(1e-4), margin_rate: None }.apply(&mut p);
        assert_eq!(p.commission_coeff_pervol, 0.0);
        assert!((p.calc_commission(900.0, 1.0) - 900.0 * 20.0 * 1e-4).abs() < 1e-9);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// QIFI 字段校正(qars 是共享库不改,在扩展层补)@yutiansut @quantaxis
// ═══════════════════════════════════════════════════════════════════════
//
// 2026-11-08 把生产快照逐字段对照标准 QIFI(CLAUDE.md 样本)发现:
// · 成交 `trade_date_time` 恒早 8 小时:qars `parse_dt_ns` 把传入的时间串当**北京墙钟**再减 8h
//   (qars2 account.rs:455),成交路径却传的 UTC 墙钟 —— 实例:一笔成交比它所属委托的
//   `insert_date_time` 早 28,799 秒;HTTP 成交查询的 `timestamp` 同样早 8 小时。
// · 持仓 `position_price_long/short` 漂移:qars `update_pos` 平仓分支用
//   (均价×持仓 − 平仓价×手数)/(持仓 − 手数) 改剩余仓位的均价(qars2 position.rs:505/535/562),
//   每平一次漂一次;平完时分母为 0 ⇒ inf/NaN ⇒ 快照里写成 null(当天 84 个快照文件,
//   恢复时被「null → 0.0」的文本替换兜住)。实例:IH 做市账户 250 手,position_cost 对应
//   1804.82,position_price_long 却是 1096.10。

/// qars 时间串:北京墙钟 `%Y-%m-%d %H:%M:%S`(qars `parse_dt_ns` 按 UTC+8 解析)。
/// 不用 `Local`:部署机时区不是 +8 时也要对。
pub fn beijing_now_str() -> String {
    let bj = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 offset");
    chrono::Utc::now()
        .with_timezone(&bj)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

/// 持仓均价按 QIFI 定义从持仓成本反推:`position_price = position_cost / (手数 × 合约乘数)`,空仓为 0。
///
/// `position_cost` 在 qars 里平仓时按 (持仓−手数)/持仓 等比缩放、结算时按结算价重置,两种情况下都成立。
/// 只改输出字段:qars 的盯市盈亏用 position_cost、浮盈用 open_cost(qars2 position.rs:326-335),
/// 不读 position_price。
pub fn conform_position_price(pos: &mut qars::qaaccount::position::QA_Position) {
    let unit = pos.preset.unit_table as f64;
    let vl = pos.volume_long_unmut();
    let vs = pos.volume_short_unmut();
    pos.position_price_long = if vl > 0.0 && unit > 0.0 {
        pos.position_cost_long / (vl * unit)
    } else {
        0.0
    };
    pos.position_price_short = if vs > 0.0 && unit > 0.0 {
        pos.position_cost_short / (vs * unit)
    } else {
        0.0
    };
}

/// 账户全部持仓的均价校正(快照恢复后调用)。返回值变化了的持仓数。
pub fn conform_position_prices_all(acc: &mut QA_Account) -> usize {
    let mut n = 0;
    for pos in acc.hold.values_mut() {
        let before = (pos.position_price_long.to_bits(), pos.position_price_short.to_bits());
        conform_position_price(pos);
        if before != (pos.position_price_long.to_bits(), pos.position_price_short.to_bits()) {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod qifi_conform_tests {
    use super::*;

    /// 回归:成交时间必须是真实时刻,不能早 8 小时
    #[test]
    fn trade_time_from_beijing_string_is_real_epoch() {
        let mut acc = QA_Account::new("acc_t8", "default", "acc_t8", 10_000_000.0, false, "sim");
        let o = acc
            .send_order("IH2501", 1.0, &beijing_now_str(), 2, 1800.0, "", "LIMIT")
            .expect("send_order");
        let now = chrono::Utc::now().timestamp();
        acc.receive_deal_sim(
            "IH2501".into(), 1.0, 1800.0, beijing_now_str(),
            o.order_id.clone(), "T1".into(), o.order_id.clone(), 2,
        );
        let td = acc.dailytrades["T1"].trade_date_time / 1_000_000_000;
        assert!((td - now).abs() <= 2, "trade_date_time {} vs now {}", td, now);
    }

    /// 部分平仓后剩余仓位的均价不变;平完为 0(不是 NaN)
    #[test]
    fn position_price_follows_cost_after_partial_and_full_close() {
        let mut acc = QA_Account::new("acc_pp", "default", "acc_pp", 100_000_000.0, false, "sim");
        let t = beijing_now_str();
        let fill = |acc: &mut QA_Account, vol: f64, px: f64, towards: i32, tid: &str| {
            let o = acc.send_order("IH2501", vol, &t, towards, px, "", "LIMIT").expect("send_order");
            acc.receive_deal_sim(
                "IH2501".into(), vol, px, t.clone(), o.order_id.clone(), tid.into(), o.order_id.clone(), towards,
            );
        };
        fill(&mut acc, 3.0, 1800.0, 2, "T1"); // 买开 3 手 @1800
        fill(&mut acc, 1.0, 1900.0, -3, "T2"); // 卖平 1 手 @1900
        let pos = acc.hold.get_mut("IH2501").unwrap();
        // qars 自己算的是 (1800×3 − 1900×1)/2 = 1750(被修的 bug)
        assert!((pos.position_price_long - 1750.0).abs() < 1e-6, "qars 原值 {}", pos.position_price_long);
        conform_position_price(pos);
        assert!((pos.position_price_long - 1800.0).abs() < 1e-9, "剩 2 手成本价仍是 1800: {}", pos.position_price_long);

        fill(&mut acc, 2.0, 1700.0, -3, "T3"); // 卖平剩下 2 手
        let pos = acc.hold.get_mut("IH2501").unwrap();
        conform_position_price(pos);
        assert_eq!(pos.position_price_long, 0.0);
        assert_eq!(conform_position_prices_all(&mut acc), 0, "已校正过的不再变");
    }
}
