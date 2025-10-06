//! 撮合引擎核心（独立进程）
//!
//! 设计原则：
//! 1. 单进程多线程 - 每个品种一个线程
//! 2. 零拷贝通信 - 通过 iceoryx2 接收订单和发送成交
//! 3. 无状态撮合 - 不维护账户信息，只负责订单匹配
//! 4. 内存池 - 预分配订单对象，避免 GC

use crate::matching::Orderbook;
use crate::matching::engine::InstrumentAsset;
use crate::protocol::ipc_messages::{OrderRequest, TradeReport, OrderbookSnapshot, OrderAccepted};
use dashmap::DashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use crossbeam::channel::{bounded, Sender, Receiver};

/// 撮合引擎核心
///
/// 运行在独立进程中，通过 iceoryx2 接收订单请求，发送成交回报
pub struct MatchingEngineCore {
    /// 订单簿池（每个品种独立）
    orderbooks: DashMap<String, Arc<RwLock<Orderbook<InstrumentAsset>>>>,

    /// 订单接收通道（暂时用 crossbeam，后续替换为 iceoryx2）
    order_receiver: Receiver<OrderRequest>,

    /// 成交发送通道（暂时用 crossbeam，后续替换为 iceoryx2）
    trade_sender: Sender<TradeReport>,

    /// 行情发送通道（暂时用 crossbeam，后续替换为 iceoryx2）
    market_sender: Sender<OrderbookSnapshot>,

    /// 订单确认发送通道（用于 sim 模式的 on_order_confirm）
    accepted_sender: Sender<OrderAccepted>,

    /// 运行标志
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl MatchingEngineCore {
    /// 创建撮合引擎核心
    pub fn new(
        order_receiver: Receiver<OrderRequest>,
        trade_sender: Sender<TradeReport>,
        market_sender: Sender<OrderbookSnapshot>,
        accepted_sender: Sender<OrderAccepted>,
    ) -> Self {
        Self {
            orderbooks: DashMap::new(),
            order_receiver,
            trade_sender,
            market_sender,
            accepted_sender,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 注册品种
    pub fn register_instrument(&self, instrument_id: String, init_price: f64) {
        let orderbook = Orderbook::new(
            InstrumentAsset::from_code(&instrument_id),
            init_price,
        );
        self.orderbooks.insert(instrument_id.clone(), Arc::new(RwLock::new(orderbook)));
        log::info!("Registered instrument in MatchingEngineCore: {}", instrument_id);
    }

    /// 启动撮合引擎主循环
    pub fn run(&self) {
        use std::sync::atomic::Ordering;

        self.running.store(true, Ordering::SeqCst);
        log::info!("MatchingEngineCore started");

        while self.running.load(Ordering::SeqCst) {
            // 接收订单请求
            match self.order_receiver.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(order_req) => {
                    self.process_order(order_req);
                }
                Err(_) => {
                    // 超时，继续循环
                    continue;
                }
            }
        }

        log::info!("MatchingEngineCore stopped");
    }

    /// 处理单个订单
    fn process_order(&self, order_req: OrderRequest) {
        // 1. 提取合约代码
        let instrument_id = std::str::from_utf8(&order_req.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string();

        // 2. 获取订单簿
        let orderbook = match self.orderbooks.get(&instrument_id) {
            Some(ob) => ob.clone(),
            None => {
                log::warn!("Orderbook not found for instrument: {}", instrument_id);
                return;
            }
        };

        // 3. 转换为撮合引擎订单
        let match_order = self.convert_to_match_order(&order_req);

        // 4. 执行撮合（核心操作）
        let mut ob = orderbook.write();
        let results = ob.process_order(match_order);
        drop(ob); // 尽早释放锁

        // 5. 处理撮合结果
        for result in results {
            match result {
                Ok(success) => {
                    self.handle_success(success, &order_req);
                }
                Err(failed) => {
                    log::warn!("Matching failed: {:?}", failed);
                }
            }
        }

        // 6. 发送行情快照（可选，根据需要）
        // self.publish_market_snapshot(&instrument_id, &orderbook);
    }

    /// 转换为撮合引擎订单
    fn convert_to_match_order(&self, req: &OrderRequest) -> crate::matching::orders::OrderRequest<InstrumentAsset> {
        use crate::matching::{OrderDirection, OrderType, orders};

        let instrument_id = std::str::from_utf8(&req.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string();

        let direction = if req.direction == 0 {
            OrderDirection::BUY
        } else {
            OrderDirection::SELL
        };

        let asset = InstrumentAsset::from_code(&instrument_id);

        orders::new_limit_order_request(
            asset,
            direction,
            req.price,
            req.volume,
            req.timestamp,
        )
    }

    /// 处理成功的撮合结果
    fn handle_success(&self, success: crate::matching::Success, req: &OrderRequest) {
        use crate::matching::Success;

        match success {
            Success::Filled {   price, volume, ts, .. } => {
                // 发送成交回报
                let trade = self.create_trade_report(req, price, volume, ts, 0); // 0=完全成交
                let _ = self.trade_sender.send(trade);

                log::debug!("Order filled: {:?} @ {} x {}",
                    std::str::from_utf8(&req.order_id).unwrap_or(""),
                    price, volume);
            }
            Success::PartiallyFilled {   price, volume, ts, .. } => {
                // 发送部分成交回报
                let trade = self.create_trade_report(req, price, volume, ts, 1); // 1=部分成交
                let _ = self.trade_sender.send(trade);

                log::debug!("Order partially filled: {:?} @ {} x {}",
                    std::str::from_utf8(&req.order_id).unwrap_or(""),
                    price, volume);
            }
            Success::Accepted {  ts, .. } => {
                // 发送订单确认消息（用于 sim 模式的 on_order_confirm）
                let accepted = self.create_order_accepted(req, ts);
                let _ = self.accepted_sender.send(accepted);

                log::debug!("Order accepted: {:?}",
                    std::str::from_utf8(&req.order_id).unwrap_or(""));
            }
            _ => {}
        }
    }

    /// 创建成交回报
    fn create_trade_report(
        &self,
        req: &OrderRequest,
        price: f64,
        volume: f64,
        timestamp: i64,
        fill_type: u8,
    ) -> TradeReport {
        let mut trade = TradeReport {
            trade_id: [0; 32],
            order_id: req.order_id,              // 账户订单ID（用于账户匹配，40字节UUID）
            exchange_order_id: [0; 32],          // 交易所订单ID（全局唯一）
            user_id: req.user_id,
            instrument_id: req.instrument_id,
            direction: req.direction,
            offset: req.offset,
            fill_type,
            _reserved: 0,
            price,
            volume,
            commission: price * volume * 0.0003, // 万三手续费
            timestamp,
            opposite_order_id: [0; 32],
            gateway_id: req.gateway_id,
            session_id: req.session_id,
        };

        // 生成交易所全局唯一的 trade_id
        let trade_id = format!("T{}", timestamp);
        let bytes = trade_id.as_bytes();
        let len = bytes.len().min(32);
        trade.trade_id[..len].copy_from_slice(&bytes[..len]);

        // 生成交易所全局唯一的 exchange_order_id
        // 格式：EX_{timestamp}_{合约}_{方向}
        let instrument_id = std::str::from_utf8(&req.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let direction_str = if req.direction == 0 { "B" } else { "S" };
        let exchange_order_id = format!("EX_{}_{}{}", timestamp, instrument_id, direction_str);
        let ex_bytes = exchange_order_id.as_bytes();
        let ex_len = ex_bytes.len().min(32);
        trade.exchange_order_id[..ex_len].copy_from_slice(&ex_bytes[..ex_len]);

        trade
    }

    /// 创建订单确认消息
    fn create_order_accepted(
        &self,
        req: &OrderRequest,
        timestamp: i64,
    ) -> OrderAccepted {
        let mut accepted = OrderAccepted {
            order_id: req.order_id,              // 40字节UUID
            exchange_order_id: [0; 32],
            user_id: req.user_id,
            instrument_id: req.instrument_id,
            timestamp,
            gateway_id: req.gateway_id,
            session_id: req.session_id,
        };

        // 生成交易所全局唯一的 exchange_order_id
        let instrument_id = std::str::from_utf8(&req.instrument_id)
            .unwrap_or("")
            .trim_end_matches('\0');
        let direction_str = if req.direction == 0 { "B" } else { "S" };
        let exchange_order_id = format!("EX_{}_{}{}", timestamp, instrument_id, direction_str);
        let ex_bytes = exchange_order_id.as_bytes();
        let ex_len = ex_bytes.len().min(32);
        accepted.exchange_order_id[..ex_len].copy_from_slice(&ex_bytes[..ex_len]);

        accepted
    }

    /// 停止撮合引擎
    pub fn stop(&self) {
        use std::sync::atomic::Ordering;
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::channel::unbounded;

    #[test]
    fn test_matching_engine_core_creation() {
        let (order_tx, order_rx) = unbounded();
        let (trade_tx, trade_rx) = unbounded();
        let (market_tx, market_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let engine = MatchingEngineCore::new(order_rx, trade_tx, market_tx, accepted_tx);
        engine.register_instrument("IX2401".to_string(), 100.0);

        assert_eq!(engine.orderbooks.len(), 1);
    }

    #[test]
    fn test_order_processing() {
        let (order_tx, order_rx) = unbounded();
        let (trade_tx, trade_rx) = unbounded();
        let (market_tx, market_rx) = unbounded();
        let (accepted_tx, accepted_rx) = unbounded();

        let engine = MatchingEngineCore::new(order_rx, trade_tx, market_tx, accepted_tx);
        engine.register_instrument("IX2401".to_string(), 100.0);

        // 创建测试订单
        let order_req = OrderRequest::new(
            "ORDER001",
            "user_01",
            "IX2401",
            crate::protocol::ipc_messages::OrderDirection::BUY,
            crate::protocol::ipc_messages::OrderOffset::OPEN,
            100.0,
            10.0,
        );

        engine.process_order(order_req);

        // 验证是否产生成交回报（如果有对手盘）
        // 由于是第一个订单，不会立即成交
    }
}
