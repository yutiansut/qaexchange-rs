//! 行情推送系统

pub struct MarketPublisher {}

impl MarketPublisher {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MarketPublisher {
    fn default() -> Self {
        Self::new()
    }
}
