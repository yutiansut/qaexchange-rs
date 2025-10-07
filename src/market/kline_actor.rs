//! K线聚合Actor
//!
//! 独立的Actix Actor，负责K线实时聚合和历史查询
//! 通过消息传递与交易系统解耦，避免阻塞主流程
//!
//! @yutiansut @quantaxis

use actix::{Actor, Context, Handler, Message, Addr};
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;

use super::kline::{KLine, KLinePeriod, KLineAggregator};
use super::MarketDataBroadcaster;
use super::MarketDataEvent;

/// K线Actor - 独立处理K线聚合，避免阻塞交易流程
pub struct KLineActor {
    /// 各合约的K线聚合器
    aggregators: Arc<RwLock<HashMap<String, KLineAggregator>>>,

    /// 市场数据广播器（用于推送K线完成事件）
    broadcaster: Option<Arc<MarketDataBroadcaster>>,
}

impl KLineActor {
    /// 创建新的K线Actor
    pub fn new() -> Self {
        Self {
            aggregators: Arc::new(RwLock::new(HashMap::new())),
            broadcaster: None,
        }
    }

    /// 设置市场数据广播器
    pub fn with_broadcaster(mut self, broadcaster: Arc<MarketDataBroadcaster>) -> Self {
        self.broadcaster = Some(broadcaster);
        self
    }
}

impl Actor for KLineActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!("📊 KLineActor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("📊 KLineActor stopped");
    }
}

/// 成交消息 - 触发K线更新
#[derive(Message)]
#[rtype(result = "()")]
pub struct OnTrade {
    pub instrument_id: String,
    pub price: f64,
    pub volume: i64,
    pub timestamp_ms: i64,
}

impl Handler<OnTrade> for KLineActor {
    type Result = ();

    fn handle(&mut self, msg: OnTrade, _ctx: &mut Context<Self>) -> Self::Result {
        let mut aggregators = self.aggregators.write();

        let aggregator = aggregators
            .entry(msg.instrument_id.clone())
            .or_insert_with(|| KLineAggregator::new(msg.instrument_id.clone()));

        // 处理Tick，获取完成的K线
        let finished_klines = aggregator.on_tick(msg.price, msg.volume, msg.timestamp_ms);

        // 广播完成的K线
        if let Some(ref broadcaster) = self.broadcaster {
            for (period, kline) in finished_klines {
                log::debug!(
                    "📊 [KLineActor] Finished {} {:?} K-line: O={:.2} H={:.2} L={:.2} C={:.2} V={}",
                    msg.instrument_id, period, kline.open, kline.high, kline.low, kline.close, kline.volume
                );

                // 广播K线完成事件
                let event = MarketDataEvent::KLineFinished {
                    instrument_id: msg.instrument_id.clone(),
                    period: period.to_int(),
                    kline: kline.clone(),
                    timestamp: msg.timestamp_ms,
                };

                broadcaster.broadcast(event);
            }
        }
    }
}

/// 查询K线消息 - 用于HTTP API和DIFF set_chart
#[derive(Message)]
#[rtype(result = "Vec<KLine>")]
pub struct GetKLines {
    pub instrument_id: String,
    pub period: KLinePeriod,
    pub count: usize,
}

impl Handler<GetKLines> for KLineActor {
    type Result = Vec<KLine>;

    fn handle(&mut self, msg: GetKLines, _ctx: &mut Context<Self>) -> Self::Result {
        let aggregators = self.aggregators.read();

        if let Some(aggregator) = aggregators.get(&msg.instrument_id) {
            aggregator.get_recent_klines(msg.period, msg.count)
        } else {
            Vec::new()
        }
    }
}

/// 获取当前K线消息（未完成的K线）
#[derive(Message)]
#[rtype(result = "Option<KLine>")]
pub struct GetCurrentKLine {
    pub instrument_id: String,
    pub period: KLinePeriod,
}

impl Handler<GetCurrentKLine> for KLineActor {
    type Result = Option<KLine>;

    fn handle(&mut self, msg: GetCurrentKLine, _ctx: &mut Context<Self>) -> Self::Result {
        let aggregators = self.aggregators.read();

        aggregators.get(&msg.instrument_id)
            .and_then(|agg| agg.get_current_kline(msg.period))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::System;

    #[test]
    fn test_kline_actor_creation() {
        let actor = KLineActor::new();
        assert!(actor.aggregators.read().is_empty());
    }

    #[actix::test]
    async fn test_kline_actor_on_trade() {
        let actor = KLineActor::new();
        let addr = actor.start();

        let now = chrono::Utc::now().timestamp_millis();

        // 发送成交消息
        addr.send(OnTrade {
            instrument_id: "IF2501".to_string(),
            price: 3800.0,
            volume: 10,
            timestamp_ms: now,
        }).await.unwrap();

        // 查询K线
        let klines = addr.send(GetKLines {
            instrument_id: "IF2501".to_string(),
            period: KLinePeriod::Min1,
            count: 10,
        }).await.unwrap();

        assert_eq!(klines.len(), 1); // 只有当前未完成的K线
    }
}
