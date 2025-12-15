//! 交易状态机实现
//!
//! 管理交易所的交易时段、状态转换和订单处理规则。
//! @yutiansut @quantaxis

use chrono::{Local, NaiveTime};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub use crate::matching::TradingState;

/// 交易时段定义
#[derive(Debug, Clone)]
pub struct TradingSession {
    /// 时段名称
    pub name: String,
    /// 开始时间 (HH:MM:SS)
    pub start_time: NaiveTime,
    /// 结束时间 (HH:MM:SS)
    pub end_time: NaiveTime,
    /// 该时段的交易状态
    pub state: TradingState,
    /// 是否允许下单
    pub allow_order: bool,
    /// 是否允许撤单
    pub allow_cancel: bool,
    /// 是否进行撮合
    pub allow_match: bool,
}

impl TradingSession {
    pub fn new(
        name: &str,
        start: &str,
        end: &str,
        state: TradingState,
        allow_order: bool,
        allow_cancel: bool,
        allow_match: bool,
    ) -> Self {
        Self {
            name: name.to_string(),
            start_time: NaiveTime::parse_from_str(start, "%H:%M:%S").unwrap(),
            end_time: NaiveTime::parse_from_str(end, "%H:%M:%S").unwrap(),
            state,
            allow_order,
            allow_cancel,
            allow_match,
        }
    }

    /// 检查当前时间是否在此时段内
    pub fn contains(&self, time: NaiveTime) -> bool {
        if self.start_time <= self.end_time {
            time >= self.start_time && time < self.end_time
        } else {
            // 跨午夜的时段
            time >= self.start_time || time < self.end_time
        }
    }
}

/// 交易所类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExchangeType {
    /// 中金所 (股指期货)
    CFFEX,
    /// 上期所 (商品期货)
    SHFE,
    /// 大商所 (商品期货)
    DCE,
    /// 郑商所 (商品期货)
    CZCE,
    /// 上海能源交易中心 (原油期货)
    INE,
    /// 模拟交易所
    SIM,
}

impl ExchangeType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "CFFEX" => Some(Self::CFFEX),
            "SHFE" => Some(Self::SHFE),
            "DCE" => Some(Self::DCE),
            "CZCE" => Some(Self::CZCE),
            "INE" => Some(Self::INE),
            "SIM" | "SIMULATED" => Some(Self::SIM),
            _ => None,
        }
    }

    /// 从合约ID中提取交易所类型
    pub fn from_instrument_id(instrument_id: &str) -> Option<Self> {
        let parts: Vec<&str> = instrument_id.split('.').collect();
        if parts.len() >= 2 {
            Self::from_str(parts[0])
        } else {
            // 尝试从合约代码推断
            let code = instrument_id.to_uppercase();
            if code.starts_with("IF") || code.starts_with("IC") || code.starts_with("IH")
                || code.starts_with("IM") || code.starts_with("T") || code.starts_with("TF")
                || code.starts_with("TL") || code.starts_with("TS")
            {
                Some(Self::CFFEX)
            } else if code.starts_with("SC") || code.starts_with("LU") || code.starts_with("BC") {
                Some(Self::INE)
            } else {
                // 默认模拟
                Some(Self::SIM)
            }
        }
    }
}

/// 交易日历配置
#[derive(Debug, Clone)]
pub struct TradingCalendar {
    /// 交易所类型 -> 交易时段列表
    sessions: HashMap<ExchangeType, Vec<TradingSession>>,
}

impl TradingCalendar {
    pub fn new() -> Self {
        let mut sessions = HashMap::new();

        // CFFEX 股指期货交易时间
        sessions.insert(
            ExchangeType::CFFEX,
            vec![
                TradingSession::new(
                    "开盘集合竞价申报",
                    "09:25:00",
                    "09:29:00",
                    TradingState::AuctionOrder,
                    true,
                    false,
                    false,
                ),
                TradingSession::new(
                    "开盘集合竞价撮合",
                    "09:29:00",
                    "09:30:00",
                    TradingState::AuctionMatch,
                    false,
                    false,
                    true,
                ),
                TradingSession::new(
                    "上午连续交易",
                    "09:30:00",
                    "11:30:00",
                    TradingState::ContinuousTrading,
                    true,
                    true,
                    true,
                ),
                TradingSession::new(
                    "午休",
                    "11:30:00",
                    "13:00:00",
                    TradingState::Closed,
                    false,
                    false,
                    false,
                ),
                TradingSession::new(
                    "下午连续交易",
                    "13:00:00",
                    "15:00:00",
                    TradingState::ContinuousTrading,
                    true,
                    true,
                    true,
                ),
                TradingSession::new(
                    "收盘集合竞价",
                    "15:00:00",
                    "15:15:00",
                    TradingState::AuctionOrder,
                    true,
                    false,
                    false,
                ),
            ],
        );

        // SHFE/DCE/CZCE 商品期货交易时间 (简化版，不含夜盘)
        let commodity_sessions = vec![
            TradingSession::new(
                "开盘集合竞价",
                "08:55:00",
                "09:00:00",
                TradingState::AuctionOrder,
                true,
                false,
                false,
            ),
            TradingSession::new(
                "上午第一节",
                "09:00:00",
                "10:15:00",
                TradingState::ContinuousTrading,
                true,
                true,
                true,
            ),
            TradingSession::new(
                "上午休息",
                "10:15:00",
                "10:30:00",
                TradingState::Closed,
                false,
                false,
                false,
            ),
            TradingSession::new(
                "上午第二节",
                "10:30:00",
                "11:30:00",
                TradingState::ContinuousTrading,
                true,
                true,
                true,
            ),
            TradingSession::new(
                "午休",
                "11:30:00",
                "13:30:00",
                TradingState::Closed,
                false,
                false,
                false,
            ),
            TradingSession::new(
                "下午交易",
                "13:30:00",
                "15:00:00",
                TradingState::ContinuousTrading,
                true,
                true,
                true,
            ),
            TradingSession::new(
                "夜盘交易",
                "21:00:00",
                "23:00:00",
                TradingState::ContinuousTrading,
                true,
                true,
                true,
            ),
        ];

        sessions.insert(ExchangeType::SHFE, commodity_sessions.clone());
        sessions.insert(ExchangeType::DCE, commodity_sessions.clone());
        sessions.insert(ExchangeType::CZCE, commodity_sessions.clone());
        sessions.insert(ExchangeType::INE, commodity_sessions);

        // SIM 模拟交易所 - 24小时开放
        sessions.insert(
            ExchangeType::SIM,
            vec![TradingSession::new(
                "全天交易",
                "00:00:00",
                "23:59:59",
                TradingState::ContinuousTrading,
                true,
                true,
                true,
            )],
        );

        Self { sessions }
    }

    /// 获取当前时段
    pub fn get_current_session(
        &self,
        exchange: ExchangeType,
        time: NaiveTime,
    ) -> Option<&TradingSession> {
        self.sessions
            .get(&exchange)?
            .iter()
            .find(|s| s.contains(time))
    }

    /// 获取当前交易状态
    pub fn get_current_state(&self, exchange: ExchangeType, time: NaiveTime) -> TradingState {
        self.get_current_session(exchange, time)
            .map(|s| s.state)
            .unwrap_or(TradingState::Closed)
    }
}

impl Default for TradingCalendar {
    fn default() -> Self {
        Self::new()
    }
}

/// 订单验证结果
#[derive(Debug, Clone)]
pub enum OrderValidation {
    /// 允许
    Allowed,
    /// 拒绝，附带原因
    Rejected(String),
}

/// 交易状态机
pub struct TradingStateMachine {
    /// 交易日历
    calendar: TradingCalendar,
    /// 当前全局状态
    global_state: RwLock<TradingState>,
    /// 合约级别状态覆盖 (instrument_id -> state)
    instrument_states: DashMap<String, TradingState>,
    /// 状态变更监听器
    state_listeners: RwLock<Vec<Box<dyn Fn(&str, TradingState, TradingState) + Send + Sync>>>,
    /// 是否启用自动状态切换
    auto_transition_enabled: AtomicBool,
    /// 是否正在运行
    running: AtomicBool,
}

impl TradingStateMachine {
    pub fn new() -> Self {
        Self {
            calendar: TradingCalendar::new(),
            global_state: RwLock::new(TradingState::Closed),
            instrument_states: DashMap::new(),
            state_listeners: RwLock::new(Vec::new()),
            auto_transition_enabled: AtomicBool::new(false),
            running: AtomicBool::new(false),
        }
    }

    /// 启动自动状态切换
    pub fn start_auto_transition(self: Arc<Self>) {
        if self
            .auto_transition_enabled
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            log::warn!("Auto transition already enabled");
            return;
        }

        self.running.store(true, Ordering::SeqCst);

        let machine = self.clone();
        thread::spawn(move || {
            log::info!("Trading state machine auto-transition started");

            while machine.running.load(Ordering::SeqCst) {
                let now = Local::now().time();

                // 更新每个交易所的状态
                for exchange_type in [
                    ExchangeType::CFFEX,
                    ExchangeType::SHFE,
                    ExchangeType::DCE,
                    ExchangeType::CZCE,
                    ExchangeType::INE,
                ] {
                    let new_state = machine.calendar.get_current_state(exchange_type, now);
                    machine.update_exchange_state(exchange_type, new_state);
                }

                // 每秒检查一次
                thread::sleep(Duration::from_secs(1));
            }

            log::info!("Trading state machine auto-transition stopped");
        });
    }

    /// 停止自动状态切换
    pub fn stop_auto_transition(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.auto_transition_enabled.store(false, Ordering::SeqCst);
    }

    /// 更新交易所状态
    fn update_exchange_state(&self, exchange: ExchangeType, new_state: TradingState) {
        let old_state = *self.global_state.read();
        if old_state != new_state {
            *self.global_state.write() = new_state;
            log::info!(
                "[{:?}] Trading state changed: {:?} -> {:?}",
                exchange,
                old_state,
                new_state
            );

            // 通知监听器
            let listeners = self.state_listeners.read();
            for listener in listeners.iter() {
                listener(&format!("{:?}", exchange), old_state, new_state);
            }
        }
    }

    /// 手动设置全局状态
    pub fn set_global_state(&self, state: TradingState) {
        let old_state = *self.global_state.read();
        *self.global_state.write() = state;
        log::info!(
            "Global trading state manually set: {:?} -> {:?}",
            old_state,
            state
        );
    }

    /// 获取全局状态
    pub fn get_global_state(&self) -> TradingState {
        *self.global_state.read()
    }

    /// 设置合约级别状态
    pub fn set_instrument_state(&self, instrument_id: &str, state: TradingState) {
        let old_state = self
            .instrument_states
            .get(instrument_id)
            .map(|r| *r)
            .unwrap_or(TradingState::Closed);
        self.instrument_states.insert(instrument_id.to_string(), state);
        log::info!(
            "Instrument {} state set: {:?} -> {:?}",
            instrument_id,
            old_state,
            state
        );
    }

    /// 获取合约的交易状态
    pub fn get_instrument_state(&self, instrument_id: &str) -> TradingState {
        // 优先返回合约级别状态，否则根据交易所类型返回
        if let Some(state) = self.instrument_states.get(instrument_id) {
            return *state;
        }

        // 根据交易所类型判断
        if let Some(exchange) = ExchangeType::from_instrument_id(instrument_id) {
            let now = Local::now().time();
            self.calendar.get_current_state(exchange, now)
        } else {
            *self.global_state.read()
        }
    }

    /// 验证订单是否允许提交
    pub fn validate_order(&self, instrument_id: &str) -> OrderValidation {
        let state = self.get_instrument_state(instrument_id);
        let exchange = ExchangeType::from_instrument_id(instrument_id);
        let now = Local::now().time();

        // 获取当前时段
        if let Some(ex) = exchange {
            if let Some(session) = self.calendar.get_current_session(ex, now) {
                if !session.allow_order {
                    return OrderValidation::Rejected(format!(
                        "当前时段({})不允许下单，交易状态: {:?}",
                        session.name, state
                    ));
                }
            }
        }

        match state {
            TradingState::ContinuousTrading => OrderValidation::Allowed,
            TradingState::AuctionOrder | TradingState::PreAuctionPeriod => OrderValidation::Allowed,
            TradingState::AuctionCancel => {
                OrderValidation::Rejected("集合竞价撤单期不允许下单".to_string())
            }
            TradingState::AuctionMatch => {
                OrderValidation::Rejected("集合竞价撮合期不允许下单".to_string())
            }
            TradingState::Closed => OrderValidation::Rejected("市场已闭市".to_string()),
        }
    }

    /// 验证撤单是否允许
    pub fn validate_cancel(&self, instrument_id: &str) -> OrderValidation {
        let state = self.get_instrument_state(instrument_id);
        let exchange = ExchangeType::from_instrument_id(instrument_id);
        let now = Local::now().time();

        // 获取当前时段
        if let Some(ex) = exchange {
            if let Some(session) = self.calendar.get_current_session(ex, now) {
                if !session.allow_cancel {
                    return OrderValidation::Rejected(format!(
                        "当前时段({})不允许撤单，交易状态: {:?}",
                        session.name, state
                    ));
                }
            }
        }

        match state {
            TradingState::ContinuousTrading => OrderValidation::Allowed,
            TradingState::PreAuctionPeriod | TradingState::AuctionCancel => OrderValidation::Allowed,
            TradingState::AuctionOrder => {
                OrderValidation::Rejected("集合竞价申报期不允许撤单".to_string())
            }
            TradingState::AuctionMatch => {
                OrderValidation::Rejected("集合竞价撮合期不允许撤单".to_string())
            }
            TradingState::Closed => OrderValidation::Rejected("市场已闭市".to_string()),
        }
    }

    /// 检查是否应该进行撮合
    pub fn should_match(&self, instrument_id: &str) -> bool {
        let state = self.get_instrument_state(instrument_id);
        matches!(
            state,
            TradingState::ContinuousTrading | TradingState::AuctionMatch
        )
    }

    /// 添加状态变更监听器
    pub fn add_listener<F>(&self, listener: F)
    where
        F: Fn(&str, TradingState, TradingState) + Send + Sync + 'static,
    {
        self.state_listeners.write().push(Box::new(listener));
    }

    /// 获取交易日历
    pub fn get_calendar(&self) -> &TradingCalendar {
        &self.calendar
    }

    /// 检查当前是否为交易时间
    pub fn is_trading_time(&self, exchange: ExchangeType) -> bool {
        let now = Local::now().time();
        matches!(
            self.calendar.get_current_state(exchange, now),
            TradingState::ContinuousTrading | TradingState::AuctionOrder
        )
    }

    /// 获取下一个状态转换时间
    pub fn get_next_transition_time(&self, exchange: ExchangeType) -> Option<NaiveTime> {
        let now = Local::now().time();
        if let Some(sessions) = self.calendar.sessions.get(&exchange) {
            for session in sessions {
                if session.start_time > now {
                    return Some(session.start_time);
                }
            }
            // 如果没有找到，返回第一个时段（次日）
            sessions.first().map(|s| s.start_time)
        } else {
            None
        }
    }
}

impl Default for TradingStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trading_session_contains() {
        let session = TradingSession::new(
            "Test",
            "09:30:00",
            "11:30:00",
            TradingState::ContinuousTrading,
            true,
            true,
            true,
        );

        assert!(session.contains(NaiveTime::from_hms_opt(9, 30, 0).unwrap()));
        assert!(session.contains(NaiveTime::from_hms_opt(10, 0, 0).unwrap()));
        assert!(!session.contains(NaiveTime::from_hms_opt(11, 30, 0).unwrap()));
        assert!(!session.contains(NaiveTime::from_hms_opt(9, 0, 0).unwrap()));
    }

    #[test]
    fn test_exchange_type_from_instrument() {
        assert_eq!(
            ExchangeType::from_instrument_id("CFFEX.IF2401"),
            Some(ExchangeType::CFFEX)
        );
        assert_eq!(
            ExchangeType::from_instrument_id("SHFE.cu2501"),
            Some(ExchangeType::SHFE)
        );
        assert_eq!(
            ExchangeType::from_instrument_id("IF2401"),
            Some(ExchangeType::CFFEX)
        );
    }

    #[test]
    fn test_trading_calendar() {
        let calendar = TradingCalendar::new();

        // CFFEX 上午连续交易期
        let state =
            calendar.get_current_state(ExchangeType::CFFEX, NaiveTime::from_hms_opt(10, 0, 0).unwrap());
        assert_eq!(state, TradingState::ContinuousTrading);

        // CFFEX 午休
        let state =
            calendar.get_current_state(ExchangeType::CFFEX, NaiveTime::from_hms_opt(12, 0, 0).unwrap());
        assert_eq!(state, TradingState::Closed);

        // SIM 全天开放
        let state =
            calendar.get_current_state(ExchangeType::SIM, NaiveTime::from_hms_opt(3, 0, 0).unwrap());
        assert_eq!(state, TradingState::ContinuousTrading);
    }

    #[test]
    fn test_state_machine_validate_order() {
        let machine = TradingStateMachine::new();

        // 手动设置为连续交易状态
        machine.set_instrument_state("TEST2401", TradingState::ContinuousTrading);
        assert!(matches!(
            machine.validate_order("TEST2401"),
            OrderValidation::Allowed
        ));

        // 设置为闭市状态
        machine.set_instrument_state("TEST2401", TradingState::Closed);
        assert!(matches!(
            machine.validate_order("TEST2401"),
            OrderValidation::Rejected(_)
        ));
    }

    #[test]
    fn test_state_machine_validate_cancel() {
        let machine = TradingStateMachine::new();

        // 连续交易期可撤单
        machine.set_instrument_state("TEST2401", TradingState::ContinuousTrading);
        assert!(matches!(
            machine.validate_cancel("TEST2401"),
            OrderValidation::Allowed
        ));

        // 集合竞价申报期不可撤单
        machine.set_instrument_state("TEST2401", TradingState::AuctionOrder);
        assert!(matches!(
            machine.validate_cancel("TEST2401"),
            OrderValidation::Rejected(_)
        ));
    }
}
