//! 订单路由模块

use crate::ExchangeError;

pub struct OrderRouter {
    // TODO: 实现完整的订单路由逻辑
}

impl OrderRouter {
    pub fn new() -> Self {
        Self {}
    }

    pub fn route_order(&self, _order_id: &str) -> Result<(), ExchangeError> {
        // TODO: 实现
        Ok(())
    }
}

impl Default for OrderRouter {
    fn default() -> Self {
        Self::new()
    }
}
