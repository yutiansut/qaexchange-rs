//! 条件单引擎
//! @yutiansut @quantaxis
//!
//! 提供止损、止盈、触价等条件单功能：
//! - 条件单的创建、查询、取消
//! - 实时行情监控和条件触发
//! - 触发后自动转为普通订单

use chrono::Utc;
use dashmap::DashMap;
use log;
use std::sync::Arc;
use uuid::Uuid;

use crate::exchange::order_router::{OrderRouter, SubmitOrderRequest};
use crate::service::http::models::{
    ConditionType, ConditionalOrderInfo, ConditionalOrderStatus,
    CreateConditionalOrderRequest, TriggerCondition,
};

/// 内部条件单结构
#[derive(Debug, Clone)]
pub struct ConditionalOrder {
    pub id: String,
    pub account_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: f64,
    pub order_type: String,
    pub limit_price: Option<f64>,
    pub condition_type: ConditionType,
    pub trigger_price: f64,
    pub trigger_condition: TriggerCondition,
    pub valid_until: Option<i64>,
    pub status: ConditionalOrderStatus,
    pub created_at: i64,
    pub triggered_at: Option<i64>,
    pub result_order_id: Option<String>,
}

impl ConditionalOrder {
    /// 转换为 API 响应格式
    pub fn to_info(&self) -> ConditionalOrderInfo {
        ConditionalOrderInfo {
            conditional_order_id: self.id.clone(),
            account_id: self.account_id.clone(),
            instrument_id: self.instrument_id.clone(),
            direction: self.direction.clone(),
            offset: self.offset.clone(),
            volume: self.volume,
            order_type: self.order_type.clone(),
            limit_price: self.limit_price,
            condition_type: self.condition_type.clone(),
            trigger_price: self.trigger_price,
            trigger_condition: self.trigger_condition.clone(),
            valid_until: self.valid_until,
            status: self.status.clone(),
            created_at: self.created_at,
            triggered_at: self.triggered_at,
            result_order_id: self.result_order_id.clone(),
        }
    }

    /// 检查是否触发
    pub fn check_trigger(&self, last_price: f64) -> bool {
        match self.trigger_condition {
            TriggerCondition::GreaterOrEqual => last_price >= self.trigger_price,
            TriggerCondition::LessOrEqual => last_price <= self.trigger_price,
        }
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(valid_until) = self.valid_until {
            Utc::now().timestamp_millis() > valid_until
        } else {
            false
        }
    }

    /// 转换为订单提交请求
    pub fn to_submit_request(&self) -> SubmitOrderRequest {
        SubmitOrderRequest {
            account_id: self.account_id.clone(),
            instrument_id: self.instrument_id.clone(),
            direction: self.direction.clone(),
            offset: self.offset.clone(),
            volume: self.volume,
            price: self.limit_price.unwrap_or(0.0),
            order_type: self.order_type.clone(),
            time_condition: None,
            volume_condition: None,
        }
    }
}

/// 条件单引擎
pub struct ConditionalOrderEngine {
    /// 条件单存储: conditional_order_id -> ConditionalOrder
    orders: DashMap<String, ConditionalOrder>,
    /// 按账户索引: account_id -> Vec<conditional_order_id>
    by_account: DashMap<String, Vec<String>>,
    /// 按合约索引: instrument_id -> Vec<conditional_order_id>
    by_instrument: DashMap<String, Vec<String>>,
    /// 订单路由器
    order_router: Option<Arc<OrderRouter>>,
}

impl ConditionalOrderEngine {
    pub fn new() -> Self {
        Self {
            orders: DashMap::new(),
            by_account: DashMap::new(),
            by_instrument: DashMap::new(),
            order_router: None,
        }
    }

    /// 设置订单路由器
    pub fn set_order_router(&mut self, router: Arc<OrderRouter>) {
        self.order_router = Some(router);
    }

    /// 创建条件单
    pub fn create_order(&self, req: CreateConditionalOrderRequest) -> Result<ConditionalOrderInfo, String> {
        let order_id = format!("COND_{}", Uuid::new_v4().to_string().replace("-", "")[..12].to_uppercase());
        let now = Utc::now().timestamp_millis();

        let order = ConditionalOrder {
            id: order_id.clone(),
            account_id: req.account_id.clone(),
            instrument_id: req.instrument_id.clone(),
            direction: req.direction,
            offset: req.offset,
            volume: req.volume,
            order_type: req.order_type,
            limit_price: req.limit_price,
            condition_type: req.condition_type,
            trigger_price: req.trigger_price,
            trigger_condition: req.trigger_condition,
            valid_until: req.valid_until,
            status: ConditionalOrderStatus::Pending,
            created_at: now,
            triggered_at: None,
            result_order_id: None,
        };

        // 添加到索引
        self.by_account
            .entry(req.account_id.clone())
            .or_default()
            .push(order_id.clone());

        self.by_instrument
            .entry(req.instrument_id)
            .or_default()
            .push(order_id.clone());

        let info = order.to_info();
        self.orders.insert(order_id, order);

        log::info!("条件单创建成功: {}", info.conditional_order_id);
        Ok(info)
    }

    /// 查询条件单
    pub fn get_order(&self, order_id: &str) -> Option<ConditionalOrderInfo> {
        self.orders.get(order_id).map(|o| o.to_info())
    }

    /// 查询账户的所有条件单
    pub fn get_orders_by_account(&self, account_id: &str) -> Vec<ConditionalOrderInfo> {
        self.by_account
            .get(account_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.orders.get(id).map(|o| o.to_info()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 取消条件单
    pub fn cancel_order(&self, order_id: &str) -> Result<(), String> {
        let mut order = self.orders.get_mut(order_id)
            .ok_or_else(|| format!("条件单不存在: {}", order_id))?;

        if order.status != ConditionalOrderStatus::Pending {
            return Err(format!("条件单状态不允许取消: {:?}", order.status));
        }

        order.status = ConditionalOrderStatus::Cancelled;
        log::info!("条件单取消成功: {}", order_id);
        Ok(())
    }

    /// 检查合约的所有条件单并触发
    /// 返回触发的条件单列表
    pub fn check_triggers(&self, instrument_id: &str, last_price: f64) -> Vec<ConditionalOrderInfo> {
        let mut triggered = Vec::new();

        if let Some(order_ids) = self.by_instrument.get(instrument_id) {
            for order_id in order_ids.iter() {
                if let Some(mut order) = self.orders.get_mut(order_id) {
                    // 只检查待触发的条件单
                    if order.status != ConditionalOrderStatus::Pending {
                        continue;
                    }

                    // 检查是否过期
                    if order.is_expired() {
                        order.status = ConditionalOrderStatus::Expired;
                        log::info!("条件单已过期: {}", order_id);
                        continue;
                    }

                    // 检查是否触发
                    if order.check_trigger(last_price) {
                        log::info!(
                            "条件单触发: {} (触发价 {} {} 当前价 {})",
                            order_id,
                            order.trigger_price,
                            match order.trigger_condition {
                                TriggerCondition::GreaterOrEqual => "<=",
                                TriggerCondition::LessOrEqual => ">=",
                            },
                            last_price
                        );

                        // 尝试执行订单
                        if let Some(router) = &self.order_router {
                            let submit_req = order.to_submit_request();
                            let response = router.submit_order(submit_req);

                            if response.success {
                                order.status = ConditionalOrderStatus::Triggered;
                                order.triggered_at = Some(Utc::now().timestamp_millis());
                                order.result_order_id = response.order_id;
                                log::info!(
                                    "条件单执行成功: {} -> 订单 {:?}",
                                    order_id,
                                    order.result_order_id
                                );
                            } else {
                                order.status = ConditionalOrderStatus::Failed;
                                order.triggered_at = Some(Utc::now().timestamp_millis());
                                log::error!(
                                    "条件单执行失败: {} - {}",
                                    order_id,
                                    response.error_message.unwrap_or_default()
                                );
                            }
                        } else {
                            // 没有订单路由器，标记为触发但无法执行
                            order.status = ConditionalOrderStatus::Triggered;
                            order.triggered_at = Some(Utc::now().timestamp_millis());
                            log::warn!("条件单触发但无法执行（无订单路由器）: {}", order_id);
                        }

                        triggered.push(order.to_info());
                    }
                }
            }
        }

        triggered
    }

    /// 获取统计信息
    pub fn get_statistics(&self) -> ConditionalOrderStatistics {
        let mut pending = 0;
        let mut triggered = 0;
        let mut cancelled = 0;
        let mut expired = 0;
        let mut failed = 0;

        for entry in self.orders.iter() {
            match entry.status {
                ConditionalOrderStatus::Pending => pending += 1,
                ConditionalOrderStatus::Triggered => triggered += 1,
                ConditionalOrderStatus::Cancelled => cancelled += 1,
                ConditionalOrderStatus::Expired => expired += 1,
                ConditionalOrderStatus::Failed => failed += 1,
            }
        }

        ConditionalOrderStatistics {
            total: self.orders.len(),
            pending,
            triggered,
            cancelled,
            expired,
            failed,
        }
    }
}

impl Default for ConditionalOrderEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// 条件单统计信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConditionalOrderStatistics {
    pub total: usize,
    pub pending: usize,
    pub triggered: usize,
    pub cancelled: usize,
    pub expired: usize,
    pub failed: usize,
}

// 全局条件单引擎
lazy_static::lazy_static! {
    pub static ref CONDITIONAL_ORDER_ENGINE: parking_lot::RwLock<ConditionalOrderEngine> =
        parking_lot::RwLock::new(ConditionalOrderEngine::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conditional_order_trigger() {
        let engine = ConditionalOrderEngine::new();

        // 创建止损条件单
        let req = CreateConditionalOrderRequest {
            account_id: "test_account".to_string(),
            instrument_id: "SHFE.cu2501".to_string(),
            direction: "SELL".to_string(),
            offset: "CLOSE".to_string(),
            volume: 1.0,
            order_type: "MARKET".to_string(),
            limit_price: None,
            condition_type: ConditionType::StopLoss,
            trigger_price: 70000.0,
            trigger_condition: TriggerCondition::LessOrEqual,
            valid_until: None,
        };

        let order = engine.create_order(req).unwrap();
        assert_eq!(order.status, ConditionalOrderStatus::Pending);

        // 价格未达到，不触发
        let triggered = engine.check_triggers("SHFE.cu2501", 75000.0);
        assert!(triggered.is_empty());

        // 价格达到，触发
        let triggered = engine.check_triggers("SHFE.cu2501", 69000.0);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].status, ConditionalOrderStatus::Triggered);
    }

    #[test]
    fn test_conditional_order_cancel() {
        let engine = ConditionalOrderEngine::new();

        let req = CreateConditionalOrderRequest {
            account_id: "test_account".to_string(),
            instrument_id: "SHFE.cu2501".to_string(),
            direction: "SELL".to_string(),
            offset: "CLOSE".to_string(),
            volume: 1.0,
            order_type: "MARKET".to_string(),
            limit_price: None,
            condition_type: ConditionType::TakeProfit,
            trigger_price: 80000.0,
            trigger_condition: TriggerCondition::GreaterOrEqual,
            valid_until: None,
        };

        let order = engine.create_order(req).unwrap();
        let order_id = order.conditional_order_id;

        // 取消
        engine.cancel_order(&order_id).unwrap();

        let cancelled_order = engine.get_order(&order_id).unwrap();
        assert_eq!(cancelled_order.status, ConditionalOrderStatus::Cancelled);
    }
}
