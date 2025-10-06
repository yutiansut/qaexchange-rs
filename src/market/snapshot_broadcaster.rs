//! 订单簿快照广播服务
//!
//! 定期获取订单簿快照并广播给订阅者

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use crate::market::{MarketDataBroadcaster, MarketDataService, PriceLevel};
use crate::matching::engine::ExchangeMatchingEngine;

/// 订单簿快照广播服务
pub struct SnapshotBroadcastService {
    market_data_service: Arc<MarketDataService>,
    market_broadcaster: Arc<MarketDataBroadcaster>,
    matching_engine: Arc<ExchangeMatchingEngine>,
}

impl SnapshotBroadcastService {
    /// 创建快照广播服务
    pub fn new(
        matching_engine: Arc<ExchangeMatchingEngine>,
        market_broadcaster: Arc<MarketDataBroadcaster>,
    ) -> Self {
        let market_data_service = Arc::new(MarketDataService::new(matching_engine.clone()));

        Self {
            market_data_service,
            market_broadcaster,
            matching_engine,
        }
    }

    /// 启动快照广播循环
    pub async fn start(&self, interval_ms: u64) {
        let mut ticker = interval(Duration::from_millis(interval_ms));

        loop {
            ticker.tick().await;

            // 获取所有合约列表
            let instruments = self.matching_engine.get_instruments();

            // 为每个合约广播订单簿快照
            for instrument_id in instruments {
                // 检查是否有订阅者
                if self.market_broadcaster.instrument_subscriber_count(&instrument_id) == 0 {
                    continue; // 没有订阅者，跳过
                }

                // 获取订单簿快照
                match self.market_data_service.get_orderbook_snapshot(&instrument_id, 10) {
                    Ok(snapshot) => {
                        // 广播快照
                        self.market_broadcaster.broadcast_orderbook_snapshot(
                            snapshot.instrument_id,
                            snapshot.bids,
                            snapshot.asks,
                        );
                    }
                    Err(e) => {
                        log::warn!("Failed to get orderbook snapshot for {}: {}", instrument_id, e);
                    }
                }
            }
        }
    }

    /// 启动快照广播后台任务
    pub fn spawn(
        matching_engine: Arc<ExchangeMatchingEngine>,
        market_broadcaster: Arc<MarketDataBroadcaster>,
        interval_ms: u64,
    ) {
        let service = Self::new(matching_engine, market_broadcaster);

        tokio::spawn(async move {
            log::info!("Orderbook snapshot broadcaster started (interval: {}ms)", interval_ms);
            service.start(interval_ms).await;
        });
    }
}
