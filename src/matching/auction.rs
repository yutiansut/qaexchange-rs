//! 集合竞价模块
//!
//! 实现集合竞价价格计算和撮合逻辑

use serde::{Deserialize, Serialize};

/// 集合竞价结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuctionResult {
    /// 集合竞价成交价
    pub auction_price: f64,

    /// 集合竞价成交量
    pub auction_volume: f64,

    /// 理论价格
    pub theoretical_price: Option<f64>,

    /// 未成交买单数量
    pub unfilled_buy_volume: f64,

    /// 未成交卖单数量
    pub unfilled_sell_volume: f64,
}

/// 集合竞价计算器
pub struct AuctionCalculator;

impl AuctionCalculator {
    /// 计算集合竞价价格
    ///
    /// 集合竞价价格确定原则（参考上交所规则）：
    /// 1. 可实现最大成交量的价格
    /// 2. 高于该价格的买入申报与低于该价格的卖出申报全部成交的价格
    /// 3. 与该价格相同的买方或卖方至少有一方全部成交的价格
    pub fn calculate_auction_price(
        buy_orders: &[(f64, f64)], // (price, volume)
        sell_orders: &[(f64, f64)],
    ) -> Option<AuctionResult> {
        if buy_orders.is_empty() || sell_orders.is_empty() {
            return None;
        }

        // 简化实现：取买一和卖一的中间价
        // TODO: 实现完整的集合竞价算法
        let best_bid = buy_orders.iter().map(|(p, _)| p).fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let best_ask = sell_orders.iter().map(|(p, _)| p).fold(f64::INFINITY, |a, &b| a.min(b));

        if best_bid < best_ask {
            return None; // 无法成交
        }

        let auction_price = (best_bid + best_ask) / 2.0;

        // 计算可成交量
        let buy_volume: f64 = buy_orders.iter()
            .filter(|(p, _)| *p >= auction_price)
            .map(|(_, v)| v)
            .sum();

        let sell_volume: f64 = sell_orders.iter()
            .filter(|(p, _)| *p <= auction_price)
            .map(|(_, v)| v)
            .sum();

        let auction_volume = buy_volume.min(sell_volume);

        Some(AuctionResult {
            auction_price,
            auction_volume,
            theoretical_price: Some(auction_price),
            unfilled_buy_volume: buy_volume - auction_volume,
            unfilled_sell_volume: sell_volume - auction_volume,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auction_price_calculation() {
        let buy_orders = vec![
            (100.0, 1000.0),
            (99.0, 500.0),
        ];

        let sell_orders = vec![
            (98.0, 800.0),
            (97.0, 200.0),
        ];

        let result = AuctionCalculator::calculate_auction_price(&buy_orders, &sell_orders);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(result.auction_price > 0.0);
        assert!(result.auction_volume > 0.0);
    }

    #[test]
    fn test_no_match() {
        let buy_orders = vec![(90.0, 1000.0)];
        let sell_orders = vec![(100.0, 1000.0)];

        let result = AuctionCalculator::calculate_auction_price(&buy_orders, &sell_orders);
        assert!(result.is_none());
    }
}
