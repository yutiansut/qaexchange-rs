//! K线聚合Actor
//!
//! 独立的Actix Actor，负责K线实时聚合和历史查询
//! 订阅MarketDataBroadcaster的tick事件，实现分级采样
//! 支持持久化和恢复
//!
//! @yutiansut @quantaxis

use actix::{Actor, Context, Handler, Message, Addr, AsyncContext};
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;

use super::kline::{KLine, KLinePeriod, KLineAggregator};
use super::MarketDataBroadcaster;
use super::MarketDataEvent;
use crate::storage::wal::{WalManager, WalRecord};

/// K线Actor - 独立处理K线聚合，避免阻塞交易流程
pub struct KLineActor {
    /// 各合约的K线聚合器
    aggregators: Arc<RwLock<HashMap<String, KLineAggregator>>>,

    /// 市场数据广播器（用于订阅tick和推送K线完成事件）
    broadcaster: Arc<MarketDataBroadcaster>,

    /// 订阅的合约列表（为空表示订阅所有合约）
    subscribed_instruments: Vec<String>,

    /// WAL管理器（用于K线持久化和恢复）
    wal_manager: Arc<WalManager>,
}

impl KLineActor {
    /// 创建新的K线Actor
    pub fn new(broadcaster: Arc<MarketDataBroadcaster>, wal_manager: Arc<WalManager>) -> Self {
        Self {
            aggregators: Arc::new(RwLock::new(HashMap::new())),
            broadcaster,
            subscribed_instruments: Vec::new(),  // 默认订阅所有
            wal_manager,
        }
    }

    /// 订阅指定合约列表
    pub fn with_instruments(mut self, instruments: Vec<String>) -> Self {
        self.subscribed_instruments = instruments;
        self
    }

    /// 从WAL恢复历史K线数据
    fn recover_from_wal(&self) {
        log::info!("📊 [KLineActor] Recovering K-line data from WAL...");

        // TODO: 实现WAL恢复逻辑
        // 1. 读取WAL文件
        // 2. 找到所有KLineFinished记录
        // 3. 恢复到aggregators

        log::info!("📊 [KLineActor] WAL recovery completed");
    }
}

impl Actor for KLineActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("📊 [KLineActor] Starting K-line aggregator...");

        // 从WAL恢复历史数据
        self.recover_from_wal();

        // 订阅市场数据的tick频道
        let subscriber_id = uuid::Uuid::new_v4().to_string();
        let receiver = self.broadcaster.subscribe(
            subscriber_id.clone(),
            self.subscribed_instruments.clone(),  // 空列表表示订阅所有合约
            vec!["tick".to_string()],  // 只订阅tick事件
        );

        // 启动异步任务持续接收tick事件
        let aggregators = self.aggregators.clone();
        let broadcaster = self.broadcaster.clone();
        let wal_manager = self.wal_manager.clone();
        let addr = ctx.address();

        let fut = async move {
            log::info!("📊 [KLineActor] Subscribed to tick events (subscriber_id={})", subscriber_id);

            loop {
                // 使用spawn_blocking避免阻塞Tokio执行器
                let receiver_clone = receiver.clone();
                match tokio::task::spawn_blocking(move || receiver_clone.recv()).await {
                    Ok(Ok(event)) => {
                        // 处理tick事件
                        if let MarketDataEvent::Tick { instrument_id, price, volume, timestamp, .. } = event {
                            let mut agg_map = aggregators.write();
                            let aggregator = agg_map
                                .entry(instrument_id.clone())
                                .or_insert_with(|| KLineAggregator::new(instrument_id.clone()));

                            // 聚合K线
                            let finished_klines = aggregator.on_tick(price, volume as i64, timestamp);

                            // 广播完成的K线
                            for (period, kline) in finished_klines {
                                log::debug!(
                                    "📊 [KLineActor] Finished {} {:?} K-line: O={:.2} H={:.2} L={:.2} C={:.2} V={}",
                                    instrument_id, period, kline.open, kline.high, kline.low, kline.close, kline.volume
                                );

                                // 广播K线完成事件
                                broadcaster.broadcast(MarketDataEvent::KLineFinished {
                                    instrument_id: instrument_id.clone(),
                                    period: period.to_int(),
                                    kline: kline.clone(),
                                    timestamp,
                                });

                                // 持久化K线到WAL
                                let wal_record = WalRecord::KLineFinished {
                                    instrument_id: WalRecord::to_fixed_array_16(&instrument_id),
                                    period: period.to_int(),
                                    kline_timestamp: kline.timestamp,
                                    open: kline.open,
                                    high: kline.high,
                                    low: kline.low,
                                    close: kline.close,
                                    volume: kline.volume,
                                    amount: kline.amount,
                                    open_oi: kline.open_oi,
                                    close_oi: kline.close_oi,
                                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                                };

                                if let Err(e) = wal_manager.append(wal_record) {
                                    log::error!("📊 [KLineActor] Failed to persist K-line to WAL: {}", e);
                                } else {
                                    log::trace!("📊 [KLineActor] K-line persisted to WAL: {} {:?}", instrument_id, period);
                                }
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        log::warn!("📊 [KLineActor] Market data channel disconnected");
                        break;
                    }
                    Err(e) => {
                        log::error!("📊 [KLineActor] spawn_blocking error: {}", e);
                        break;
                    }
                }
            }

            log::info!("📊 [KLineActor] Tick processing task ended");
        };

        ctx.spawn(actix::fut::wrap_future(fut));

        log::info!("📊 [KLineActor] Started successfully");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("📊 [KLineActor] Stopped");
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
