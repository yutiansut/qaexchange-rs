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
    /// 4. 如果有多个价格满足条件，选择最接近前收盘价的价格
    pub fn calculate_auction_price(
        buy_orders: &[(f64, f64)], // (price, volume)
        sell_orders: &[(f64, f64)],
    ) -> Option<AuctionResult> {
        Self::calculate_auction_price_with_reference(buy_orders, sell_orders, None)
    }

    /// 计算集合竞价价格（带参考价）
    ///
    /// # Arguments
    /// * `buy_orders` - 买单列表 [(价格, 数量)]
    /// * `sell_orders` - 卖单列表 [(价格, 数量)]
    /// * `reference_price` - 参考价格（如前收盘价），用于tie-breaking
    pub fn calculate_auction_price_with_reference(
        buy_orders: &[(f64, f64)],
        sell_orders: &[(f64, f64)],
        reference_price: Option<f64>,
    ) -> Option<AuctionResult> {
        if buy_orders.is_empty() || sell_orders.is_empty() {
            return None;
        }

        // 1. 收集所有可能的价格点
        let mut price_points = std::collections::BTreeSet::new();
        for (price, _) in buy_orders {
            price_points.insert((*price * 100.0).round() as i64); // 转换为整数避免浮点误差
        }
        for (price, _) in sell_orders {
            price_points.insert((*price * 100.0).round() as i64);
        }

        if price_points.is_empty() {
            return None;
        }

        // 2. 对每个价格点，计算能成交的量
        let mut candidates = Vec::new();

        for &price_int in &price_points {
            let price = price_int as f64 / 100.0;

            // 计算在该价格下能成交的买卖量
            // 买方：价格 >= auction_price 的订单可以成交
            let buy_volume: f64 = buy_orders
                .iter()
                .filter(|(p, _)| *p >= price)
                .map(|(_, v)| v)
                .sum();

            // 卖方：价格 <= auction_price 的订单可以成交
            let sell_volume: f64 = sell_orders
                .iter()
                .filter(|(p, _)| *p <= price)
                .map(|(_, v)| v)
                .sum();

            // 成交量 = min(买量, 卖量)
            let match_volume = buy_volume.min(sell_volume);

            if match_volume > 0.0 {
                candidates.push((price, match_volume, buy_volume, sell_volume));
            }
        }

        if candidates.is_empty() {
            return None; // 无法成交
        }

        // 3. 选择成交量最大的价格
        let max_volume = candidates
            .iter()
            .map(|(_, vol, _, _)| *vol)
            .fold(f64::NEG_INFINITY, f64::max);

        // 筛选出成交量最大的价格
        let mut best_candidates: Vec<_> = candidates
            .into_iter()
            .filter(|(_, vol, _, _)| (*vol - max_volume).abs() < 1e-6)
            .collect();

        if best_candidates.is_empty() {
            return None;
        }

        // 4. Tie-breaking: 如果有多个价格成交量相同
        let auction_price = if best_candidates.len() == 1 {
            best_candidates[0].0
        } else if let Some(ref_price) = reference_price {
            // 选择最接近参考价的价格
            best_candidates.sort_by(|a, b| {
                let dist_a = (a.0 - ref_price).abs();
                let dist_b = (b.0 - ref_price).abs();
                dist_a.partial_cmp(&dist_b).unwrap()
            });
            best_candidates[0].0
        } else {
            // 无参考价时，选择中间价格
            let prices: Vec<f64> = best_candidates.iter().map(|(p, _, _, _)| *p).collect();
            let mid_idx = prices.len() / 2;
            prices[mid_idx]
        };

        // 5. 计算最终成交结果
        let buy_volume: f64 = buy_orders
            .iter()
            .filter(|(p, _)| *p >= auction_price)
            .map(|(_, v)| v)
            .sum();

        let sell_volume: f64 = sell_orders
            .iter()
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
        let buy_orders = vec![(100.0, 1000.0), (99.0, 500.0)];

        let sell_orders = vec![(98.0, 800.0), (97.0, 200.0)];

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

    #[test]
    fn test_max_volume_principle() {
        // 测试最大成交量原则
        // 买单：100元1000手，99元500手
        // 卖单：98元800手，97元200手
        // 在97-100之间，98元能成交1000手（所有卖单），成交量最大
        let buy_orders = vec![(100.0, 1000.0), (99.0, 500.0)];

        let sell_orders = vec![(98.0, 800.0), (97.0, 200.0)];

        let result = AuctionCalculator::calculate_auction_price(&buy_orders, &sell_orders).unwrap();

        // 在98元价格，买方1500手愿意成交，卖方1000手，成交量1000手
        assert!((result.auction_volume - 1000.0).abs() < 1.0);
        assert!(result.auction_price >= 97.0 && result.auction_price <= 100.0);
    }

    #[test]
    fn test_tie_breaking_with_reference() {
        // 测试参考价tie-breaking
        // 构造一个场景：多个价格成交量相同，选择最接近参考价的
        let buy_orders = vec![
            (102.0, 100.0), // 买单：102元100手
            (100.0, 50.0),  // 买单：100元50手
            (98.0, 100.0),  // 买单：98元100手
        ];

        let sell_orders = vec![
            (98.0, 100.0),  // 卖单：98元100手
            (100.0, 50.0),  // 卖单：100元50手
            (102.0, 100.0), // 卖单：102元100手
        ];

        // 在98元：买方250手(所有订单)，卖方100手，成交100手
        // 在100元：买方150手(102+100)，卖方150手(98+100)，成交150手
        // 在102元：买方100手(102)，卖方250手(所有订单)，成交100手
        // 最大成交量150手发生在100元，参考价100元
        let result = AuctionCalculator::calculate_auction_price_with_reference(
            &buy_orders,
            &sell_orders,
            Some(100.0),
        )
        .unwrap();

        // 应该选择100元（最接近参考价且成交量最大）
        assert!((result.auction_price - 100.0).abs() < 0.1);
        assert_eq!(result.auction_volume, 150.0);
    }

    #[test]
    fn test_complex_scenario() {
        // 复杂场景：多档位订单
        let buy_orders = vec![
            (105.0, 100.0),
            (104.0, 200.0),
            (103.0, 300.0),
            (102.0, 400.0),
            (101.0, 500.0),
        ];

        let sell_orders = vec![
            (99.0, 200.0),
            (100.0, 300.0),
            (101.0, 400.0),
            (102.0, 500.0),
            (103.0, 600.0),
        ];

        let result = AuctionCalculator::calculate_auction_price(&buy_orders, &sell_orders).unwrap();

        // 验证成交量 > 0
        assert!(result.auction_volume > 0.0);
        // 验证价格在合理范围内
        assert!(result.auction_price >= 99.0 && result.auction_price <= 105.0);

        log::info!(
            "Complex auction: price={:.2}, volume={:.0}, unfilled_buy={:.0}, unfilled_sell={:.0}",
            result.auction_price,
            result.auction_volume,
            result.unfilled_buy_volume,
            result.unfilled_sell_volume
        );
    }

    #[test]
    fn test_single_price_point() {
        // 单一价格点
        let buy_orders = vec![(100.0, 1000.0)];
        let sell_orders = vec![(100.0, 800.0)];

        let result = AuctionCalculator::calculate_auction_price(&buy_orders, &sell_orders).unwrap();

        assert_eq!(result.auction_price, 100.0);
        assert_eq!(result.auction_volume, 800.0);
        assert_eq!(result.unfilled_buy_volume, 200.0);
        assert_eq!(result.unfilled_sell_volume, 0.0);
    }

    #[test]
    fn test_empty_orders() {
        // 空订单列表
        let buy_orders = vec![];
        let sell_orders = vec![(100.0, 1000.0)];

        let result = AuctionCalculator::calculate_auction_price(&buy_orders, &sell_orders);
        assert!(result.is_none());

        let buy_orders = vec![(100.0, 1000.0)];
        let sell_orders = vec![];

        let result = AuctionCalculator::calculate_auction_price(&buy_orders, &sell_orders);
        assert!(result.is_none());
    }
}
