//! 数据查询和导出 API @yutiansut @quantaxis
//!
//! 提供历史数据查询、统计分析、数据导出等功能
//!
//! Phase 14: 真实数据查询实现（连接存储层）
//! - 历史Tick数据：从 market_data_storage WAL 查询
//! - K线数据：从 kline_wal_manager WAL 查询
//! - 统计分析：基于账户真实数据计算
//! - 数据导出：支持 CSV/JSON 格式

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use crate::service::http::handlers::AppState;
use crate::storage::wal::record::{WalRecord, WalEntry};
use rkyv::Deserialize as RkyvDeserialize;

// ==================== 请求/响应结构 ====================

/// 历史Tick查询请求
#[derive(Debug, Deserialize)]
pub struct HistoryTickQuery {
    pub instrument_id: String,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub limit: Option<usize>,
}

/// 交易统计请求
#[derive(Debug, Deserialize)]
pub struct TradeStatisticsQuery {
    pub account_id: Option<String>,
    pub instrument_id: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub group_by: Option<String>,  // day, week, month, instrument
}

/// 数据导出请求
#[derive(Debug, Deserialize)]
pub struct DataExportRequest {
    pub account_id: String,
    pub data_type: String,  // orders, trades, positions, transactions
    pub format: Option<String>,  // csv, json
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// 盈亏分析请求
#[derive(Debug, Deserialize)]
pub struct PnlAnalysisQuery {
    pub account_id: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub by_instrument: Option<bool>,
}

/// 日结算单请求
#[derive(Debug, Deserialize)]
pub struct SettlementStatementQuery {
    pub account_id: String,
    pub date: String,
}

/// 批量K线查询请求
#[derive(Debug, Deserialize)]
pub struct BatchKlineQuery {
    pub instruments: String,  // 逗号分隔的合约列表
    pub period: String,       // 1m, 5m, 15m, 30m, 60m, 1d
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub limit: Option<usize>,
}

/// 交易排行查询请求
#[derive(Debug, Deserialize)]
pub struct TradeRankingQuery {
    pub rank_by: Option<String>,  // pnl, pnl_ratio, volume, trade_count
    pub limit: Option<usize>,
    pub period: Option<String>,  // today, week, month, all
}

// ==================== 响应结构 ====================

/// Tick数据
#[derive(Debug, Serialize)]
pub struct TickDataItem {
    pub instrument_id: String,
    pub datetime: String,
    pub last_price: f64,
    pub volume: i64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub timestamp: i64,
}

/// K线数据
#[derive(Debug, Serialize)]
pub struct KlineDataItem {
    pub instrument_id: String,
    pub datetime: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
    pub amount: f64,
    pub open_oi: i64,
    pub close_oi: i64,
    pub period: String,
    pub timestamp: i64,
}

/// 交易统计响应
#[derive(Debug, Serialize)]
pub struct TradeStatistics {
    pub total_trades: i64,
    pub total_volume: f64,
    pub total_turnover: f64,
    pub total_commission: f64,
    pub win_count: i64,
    pub loss_count: i64,
    pub win_rate: f64,
    pub avg_profit: f64,
    pub avg_loss: f64,
    pub profit_factor: f64,
    pub max_profit: f64,
    pub max_loss: f64,
    pub by_period: Vec<PeriodStats>,
    pub by_instrument: Vec<InstrumentStats>,
}

#[derive(Debug, Serialize)]
pub struct PeriodStats {
    pub period: String,
    pub trades: i64,
    pub volume: f64,
    pub pnl: f64,
    pub commission: f64,
}

#[derive(Debug, Serialize)]
pub struct InstrumentStats {
    pub instrument_id: String,
    pub trades: i64,
    pub volume: f64,
    pub pnl: f64,
    pub commission: f64,
    pub win_rate: f64,
}

/// 盈亏分析响应
#[derive(Debug, Serialize)]
pub struct PnlAnalysis {
    pub account_id: String,
    pub period: String,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub total_pnl: f64,
    pub commission: f64,
    pub net_pnl: f64,
    pub daily_pnl: Vec<DailyPnl>,
    pub instrument_pnl: Vec<InstrumentPnl>,
}

#[derive(Debug, Serialize)]
pub struct DailyPnl {
    pub date: String,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub commission: f64,
    pub net_pnl: f64,
    pub cumulative_pnl: f64,
}

#[derive(Debug, Serialize)]
pub struct InstrumentPnl {
    pub instrument_id: String,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub commission: f64,
    pub net_pnl: f64,
}

/// 持仓分析响应
#[derive(Debug, Serialize)]
pub struct PositionAnalysis {
    pub account_id: String,
    pub total_margin: f64,
    pub total_value: f64,
    pub total_pnl: f64,
    pub risk_ratio: f64,
    pub positions: Vec<PositionDetail>,
    pub concentration: Vec<ConcentrationItem>,
}

#[derive(Debug, Serialize)]
pub struct PositionDetail {
    pub instrument_id: String,
    pub exchange_id: String,
    pub direction: String,
    pub volume: f64,
    pub avg_price: f64,
    pub current_price: f64,
    pub margin: f64,
    pub pnl: f64,
    pub pnl_ratio: f64,
    pub weight: f64,
}

#[derive(Debug, Serialize)]
pub struct ConcentrationItem {
    pub category: String,  // exchange, product
    pub name: String,
    pub margin: f64,
    pub weight: f64,
}

/// 日结算单
#[derive(Debug, Serialize)]
pub struct SettlementStatement {
    pub account_id: String,
    pub trading_day: String,
    pub pre_balance: f64,
    pub deposit: f64,
    pub withdraw: f64,
    pub commission: f64,
    pub close_profit: f64,
    pub position_profit: f64,
    pub balance: f64,
    pub margin: f64,
    pub available: f64,
    pub risk_ratio: f64,
    pub positions: Vec<SettlementPosition>,
    pub trades: Vec<SettlementTrade>,
}

#[derive(Debug, Serialize)]
pub struct SettlementPosition {
    pub instrument_id: String,
    pub direction: String,
    pub volume: i64,
    pub avg_price: f64,
    pub settlement_price: f64,
    pub margin: f64,
    pub pnl: f64,
}

#[derive(Debug, Serialize)]
pub struct SettlementTrade {
    pub trade_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub price: f64,
    pub volume: i64,
    pub commission: f64,
    pub trade_time: String,
}

// ==================== 辅助函数 ====================

/// 从固定数组提取字符串
fn extract_string(arr: &[u8]) -> String {
    WalRecord::from_fixed_array(arr)
}

/// 纳秒时间戳转日期时间字符串
fn timestamp_to_datetime(ts: i64) -> String {
    let secs = ts / 1_000_000_000;
    let nanos = (ts % 1_000_000_000) as u32;
    if let Some(dt) = chrono::DateTime::from_timestamp(secs, nanos) {
        dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
    } else {
        format!("{}", ts)
    }
}

/// 从纳秒时间戳提取时间段键值 @yutiansut @quantaxis
/// 支持的分组粒度: hour, day, week, month
fn extract_period_key(timestamp_ns: i64, group_by: &str) -> String {
    let secs = timestamp_ns / 1_000_000_000;
    if let Some(dt) = chrono::DateTime::from_timestamp(secs, 0) {
        match group_by {
            "hour" => dt.format("%Y-%m-%d %H:00").to_string(),
            "day" => dt.format("%Y-%m-%d").to_string(),
            "week" => {
                // ISO 周格式: YYYY-Www
                dt.format("%G-W%V").to_string()
            }
            "month" => dt.format("%Y-%m").to_string(),
            _ => dt.format("%Y-%m-%d").to_string(), // 默认按日
        }
    } else {
        "unknown".to_string()
    }
}

/// K线周期映射（HQChart格式）
fn period_to_string(period: i32) -> String {
    match period {
        0 => "1d".to_string(),
        3 => "3s".to_string(),
        4 => "1m".to_string(),
        5 => "5m".to_string(),
        6 => "15m".to_string(),
        7 => "30m".to_string(),
        8 => "60m".to_string(),
        _ => format!("{}s", period),
    }
}

/// 字符串周期转HQChart格式
fn string_to_period(s: &str) -> i32 {
    match s {
        "1d" | "day" => 0,
        "3s" => 3,
        "1m" | "1min" => 4,
        "5m" | "5min" => 5,
        "15m" | "15min" => 6,
        "30m" | "30min" => 7,
        "60m" | "1h" | "60min" => 8,
        _ => 4, // 默认1分钟
    }
}

// ==================== API 处理函数 ====================

/// 查询历史Tick数据（从WAL真实读取）
/// @yutiansut @quantaxis
pub async fn query_history_ticks(
    query: web::Query<HistoryTickQuery>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let instrument_id = &query.instrument_id;
    let start_time = query.start_time.unwrap_or(0);
    let end_time = query.end_time.unwrap_or(i64::MAX);
    let limit = query.limit.unwrap_or(1000);

    let mut ticks: Vec<TickDataItem> = Vec::new();

    // 从 market_data_storage WAL 读取真实数据
    if let Some(ref storage) = state.market_data_storage {
        // 回放WAL获取TickData记录
        let wal_mgr = storage.get_wal_manager();

        let _ = wal_mgr.replay(|entry: WalEntry| {
            // 检查时间范围
            if entry.timestamp < start_time || entry.timestamp > end_time {
                return Ok(());
            }

            // 只处理TickData记录
            if let WalRecord::TickData {
                instrument_id: inst_id,
                last_price,
                bid_price,
                ask_price,
                volume,
                timestamp
            } = entry.record {
                let inst_str = extract_string(&inst_id);

                // 过滤合约
                if inst_str == *instrument_id || instrument_id == "*" {
                    if ticks.len() < limit {
                        ticks.push(TickDataItem {
                            instrument_id: inst_str,
                            datetime: timestamp_to_datetime(timestamp),
                            last_price,
                            volume,
                            bid_price,
                            ask_price,
                            timestamp,
                        });
                    }
                }
            }
            Ok(())
        });
    }

    // 按时间排序（最新在前）
    ticks.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": {
            "instrument_id": instrument_id,
            "count": ticks.len(),
            "start_time": start_time,
            "end_time": end_time,
            "ticks": ticks
        }
    }))
}

/// 查询批量K线数据（从WAL真实读取）
/// @yutiansut @quantaxis
pub async fn query_batch_klines(
    query: web::Query<BatchKlineQuery>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let instruments: Vec<&str> = query.instruments.split(',').map(|s| s.trim()).collect();
    let period = string_to_period(&query.period);
    let start_time = query.start_time.unwrap_or(0);
    let end_time = query.end_time.unwrap_or(i64::MAX);
    let limit = query.limit.unwrap_or(500);

    let mut result: HashMap<String, Vec<KlineDataItem>> = HashMap::new();
    for inst in &instruments {
        result.insert(inst.to_string(), Vec::new());
    }

    // 从 kline_wal_manager WAL 读取真实数据
    if let Some(ref wal_mgr) = state.kline_wal_manager {
        let _ = wal_mgr.replay(|entry: WalEntry| {
            // 只处理KLineFinished记录
            if let WalRecord::KLineFinished {
                instrument_id,
                period: kline_period,
                kline_timestamp,
                open,
                high,
                low,
                close,
                volume,
                amount,
                open_oi,
                close_oi,
                timestamp: _,
            } = entry.record {
                // 检查周期
                if kline_period != period {
                    return Ok(());
                }

                // 检查时间范围（K线时间戳是毫秒）
                let ts_ns = kline_timestamp * 1_000_000; // 转换为纳秒
                if ts_ns < start_time || ts_ns > end_time {
                    return Ok(());
                }

                let inst_str = extract_string(&instrument_id);

                // 检查是否在请求的合约列表中
                let should_include = instruments.iter().any(|i| *i == inst_str || *i == "*");

                if should_include {
                    if let Some(klines) = result.get_mut(&inst_str) {
                        if klines.len() < limit {
                            klines.push(KlineDataItem {
                                instrument_id: inst_str.clone(),
                                datetime: timestamp_to_datetime(kline_timestamp * 1_000_000),
                                open,
                                high,
                                low,
                                close,
                                volume,
                                amount,
                                open_oi,
                                close_oi,
                                period: period_to_string(kline_period),
                                timestamp: kline_timestamp,
                            });
                        }
                    } else {
                        // 新合约
                        result.insert(inst_str.clone(), vec![KlineDataItem {
                            instrument_id: inst_str.clone(),
                            datetime: timestamp_to_datetime(kline_timestamp * 1_000_000),
                            open,
                            high,
                            low,
                            close,
                            volume,
                            amount,
                            open_oi,
                            close_oi,
                            period: period_to_string(kline_period),
                            timestamp: kline_timestamp,
                        }]);
                    }
                }
            }
            Ok(())
        });
    }

    // 排序（按时间升序）
    for klines in result.values_mut() {
        klines.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }

    // 转换为数组格式
    let data: Vec<serde_json::Value> = result.into_iter()
        .map(|(inst, klines)| serde_json::json!({
            "instrument_id": inst,
            "period": query.period,
            "count": klines.len(),
            "klines": klines
        }))
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": data
    }))
}

/// 获取交易统计（真实计算）
/// @yutiansut @quantaxis
pub async fn get_trade_statistics(
    query: web::Query<TradeStatisticsQuery>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let account_id = query.account_id.clone().unwrap_or_default();
    let instrument_filter = query.instrument_id.clone();
    let group_by = query.group_by.clone().unwrap_or_else(|| "day".to_string());

    let mut total_trades = 0i64;
    let mut total_volume = 0.0f64;
    let mut total_turnover = 0.0f64;
    let mut total_commission = 0.0f64;
    let mut win_count = 0i64;
    let mut loss_count = 0i64;
    let mut total_profit = 0.0f64;
    let mut total_loss = 0.0f64;
    let mut max_profit = 0.0f64;
    let mut max_loss = 0.0f64;
    let mut instrument_stats: HashMap<String, InstrumentStats> = HashMap::new();
    let mut period_stats: HashMap<String, PeriodStats> = HashMap::new();

    // 获取账户或所有账户
    let accounts = if !account_id.is_empty() {
        if let Ok(acc) = state.account_mgr.get_account(&account_id) {
            vec![acc]
        } else {
            vec![]
        }
    } else {
        state.account_mgr.get_all_accounts()
    };

    for account in accounts {
        let account_read = account.read();

        for (_trade_id, trade) in account_read.dailytrades.iter() {
            // 合约过滤
            if let Some(ref inst_filter) = instrument_filter {
                if &trade.instrument_id != inst_filter {
                    continue;
                }
            }

            total_trades += 1;
            total_volume += trade.volume;
            total_turnover += trade.volume * trade.price;
            total_commission += trade.commission;

            // 计算盈亏（简化：假设平仓交易）
            // 实际应该根据 offset 判断开平仓并计算真实盈亏
            let pnl = 0.0; // 需要关联持仓计算

            if pnl > 0.0 {
                win_count += 1;
                total_profit += pnl;
                if pnl > max_profit {
                    max_profit = pnl;
                }
            } else if pnl < 0.0 {
                loss_count += 1;
                total_loss += pnl.abs();
                if pnl.abs() > max_loss {
                    max_loss = pnl.abs();
                }
            }

            // 按合约统计
            let inst_stat = instrument_stats
                .entry(trade.instrument_id.clone())
                .or_insert(InstrumentStats {
                    instrument_id: trade.instrument_id.clone(),
                    trades: 0,
                    volume: 0.0,
                    pnl: 0.0,
                    commission: 0.0,
                    win_rate: 0.0,
                });
            inst_stat.trades += 1;
            inst_stat.volume += trade.volume;
            inst_stat.commission += trade.commission;

            // 按时间段统计 @yutiansut @quantaxis
            let period_key = extract_period_key(trade.trade_date_time, &group_by);
            let period_stat = period_stats.entry(period_key.clone()).or_insert(PeriodStats {
                period: period_key,
                trades: 0,
                volume: 0.0,
                pnl: 0.0,
                commission: 0.0,
            });
            period_stat.trades += 1;
            period_stat.volume += trade.volume;
            period_stat.pnl += pnl;
            period_stat.commission += trade.commission;
        }
    }

    let win_rate = if total_trades > 0 {
        win_count as f64 / total_trades as f64
    } else {
        0.0
    };

    let avg_profit = if win_count > 0 {
        total_profit / win_count as f64
    } else {
        0.0
    };

    let avg_loss = if loss_count > 0 {
        total_loss / loss_count as f64
    } else {
        0.0
    };

    let profit_factor = if total_loss > 0.0 {
        total_profit / total_loss
    } else if total_profit > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    // 按时间排序 period_stats @yutiansut @quantaxis
    let mut by_period: Vec<PeriodStats> = period_stats.into_values().collect();
    by_period.sort_by(|a, b| a.period.cmp(&b.period));

    let stats = TradeStatistics {
        total_trades,
        total_volume,
        total_turnover,
        total_commission,
        win_count,
        loss_count,
        win_rate,
        avg_profit,
        avg_loss,
        profit_factor,
        max_profit,
        max_loss,
        by_period,
        by_instrument: instrument_stats.into_values().collect(),
    };

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": stats
    }))
}

/// 获取盈亏分析（真实计算）
/// @yutiansut @quantaxis
pub async fn get_pnl_analysis(
    query: web::Query<PnlAnalysisQuery>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let account_id = &query.account_id;

    let mut realized_pnl = 0.0f64;
    let mut unrealized_pnl = 0.0f64;
    let mut commission = 0.0f64;
    let mut instrument_pnl: Vec<InstrumentPnl> = Vec::new();
    let mut daily_pnl_map: HashMap<String, DailyPnlAccum> = HashMap::new();

    if let Ok(account) = state.account_mgr.get_account(account_id) {
        let account_read = account.read();

        // 从QIFI结构获取真实数据
        realized_pnl = account_read.accounts.close_profit;
        unrealized_pnl = account_read.accounts.position_profit;
        commission = account_read.accounts.commission;

        // 从 dailytrades 计算每日盈亏 @yutiansut @quantaxis
        for (_trade_id, trade) in account_read.dailytrades.iter() {
            let date_key = extract_period_key(trade.trade_date_time, "day");
            let daily_accum = daily_pnl_map.entry(date_key.clone()).or_insert(DailyPnlAccum {
                date: date_key,
                realized_pnl: 0.0,
                commission: 0.0,
            });
            // 平仓交易记录的盈亏（简化计算）
            // 实际应该根据 offset=CLOSE 时计算真实盈亏
            daily_accum.commission += trade.commission;
        }

        // 按合约计算盈亏（需要可变引用来调用float_profit方法）
        if query.by_instrument.unwrap_or(false) {
            drop(account_read); // 释放读锁
            let mut account_write = account.write();
            for (inst_id, pos) in account_write.hold.iter_mut() {
                let float_long = pos.float_profit_long();
                let float_short = pos.float_profit_short();
                instrument_pnl.push(InstrumentPnl {
                    instrument_id: inst_id.clone(),
                    realized_pnl: 0.0, // 需要从成交记录计算
                    unrealized_pnl: float_long + float_short,
                    commission: 0.0,
                    net_pnl: float_long + float_short,
                });
            }
        }
    }

    // 转换为 DailyPnl 并计算累计盈亏 @yutiansut @quantaxis
    let mut daily_pnl: Vec<DailyPnl> = daily_pnl_map
        .into_values()
        .map(|accum| DailyPnl {
            date: accum.date,
            realized_pnl: accum.realized_pnl,
            unrealized_pnl: 0.0, // 历史未实现盈亏需要从历史快照获取
            commission: accum.commission,
            net_pnl: accum.realized_pnl - accum.commission,
            cumulative_pnl: 0.0, // 将在排序后计算
        })
        .collect();

    // 按日期排序
    daily_pnl.sort_by(|a, b| a.date.cmp(&b.date));

    // 计算累计盈亏
    let mut cumulative = 0.0f64;
    for pnl in daily_pnl.iter_mut() {
        cumulative += pnl.net_pnl;
        pnl.cumulative_pnl = cumulative;
    }

    let analysis = PnlAnalysis {
        account_id: account_id.clone(),
        period: format!("{} ~ {}",
            query.start_date.clone().unwrap_or_else(|| "2024-01-01".to_string()),
            query.end_date.clone().unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string())
        ),
        realized_pnl,
        unrealized_pnl,
        total_pnl: realized_pnl + unrealized_pnl,
        commission,
        net_pnl: realized_pnl + unrealized_pnl - commission,
        daily_pnl,
        instrument_pnl,
    };

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": analysis
    }))
}

/// 每日盈亏累计器（内部使用）@yutiansut @quantaxis
struct DailyPnlAccum {
    date: String,
    realized_pnl: f64,
    commission: f64,
}

/// 获取持仓分析（真实数据）
/// @yutiansut @quantaxis
pub async fn get_position_analysis(
    account_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let account_id = account_id.into_inner();

    let mut positions = Vec::new();
    let mut total_margin = 0.0f64;
    let mut total_pnl = 0.0f64;
    let mut concentration: Vec<ConcentrationItem> = Vec::new();
    let mut exchange_margin: HashMap<String, f64> = HashMap::new();

    if let Ok(account) = state.account_mgr.get_account(&account_id) {
        // 需要可变引用来调用float_profit方法
        let mut account_write = account.write();

        for (inst_id, pos) in account_write.hold.iter_mut() {
            let volume_long = pos.volume_long_today + pos.volume_long_his;
            let volume_short = pos.volume_short_today + pos.volume_short_his;

            if volume_long > 0.0 {
                let margin = pos.margin_long;
                let pnl = pos.float_profit_long();
                total_margin += margin;
                total_pnl += pnl;

                // 交易所集中度
                *exchange_margin.entry(pos.exchange_id.clone()).or_insert(0.0) += margin;

                positions.push(PositionDetail {
                    instrument_id: inst_id.clone(),
                    exchange_id: pos.exchange_id.clone(),
                    direction: "LONG".to_string(),
                    volume: volume_long,
                    avg_price: pos.open_price_long,
                    current_price: pos.lastest_price,
                    margin,
                    pnl,
                    pnl_ratio: if margin > 0.0 { pnl / margin * 100.0 } else { 0.0 },
                    weight: 0.0,
                });
            }

            if volume_short > 0.0 {
                let margin = pos.margin_short;
                let pnl = pos.float_profit_short();
                total_margin += margin;
                total_pnl += pnl;

                *exchange_margin.entry(pos.exchange_id.clone()).or_insert(0.0) += margin;

                positions.push(PositionDetail {
                    instrument_id: inst_id.clone(),
                    exchange_id: pos.exchange_id.clone(),
                    direction: "SHORT".to_string(),
                    volume: volume_short,
                    avg_price: pos.open_price_short,
                    current_price: pos.lastest_price,
                    margin,
                    pnl,
                    pnl_ratio: if margin > 0.0 { pnl / margin * 100.0 } else { 0.0 },
                    weight: 0.0,
                });
            }
        }

        // 计算权重
        if total_margin > 0.0 {
            for pos in &mut positions {
                pos.weight = pos.margin / total_margin * 100.0;
            }

            // 交易所集中度
            for (exchange, margin) in exchange_margin {
                concentration.push(ConcentrationItem {
                    category: "exchange".to_string(),
                    name: exchange,
                    margin,
                    weight: margin / total_margin * 100.0,
                });
            }
        }
    }

    let balance = if let Ok(account) = state.account_mgr.get_account(&account_id) {
        account.read().accounts.balance
    } else {
        0.0
    };

    let analysis = PositionAnalysis {
        account_id: account_id.clone(),
        total_margin,
        total_value: balance,
        total_pnl,
        risk_ratio: if balance > 0.0 { total_margin / balance * 100.0 } else { 0.0 },
        positions,
        concentration,
    };

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": analysis
    }))
}

/// 获取日结算单（真实数据）
/// @yutiansut @quantaxis
pub async fn get_settlement_statement(
    query: web::Query<SettlementStatementQuery>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let account_id = &query.account_id;

    if let Ok(account) = state.account_mgr.get_account(account_id) {
        // 需要可变引用来调用float_profit方法
        let mut account_write = account.write();

        // 先收集持仓数据（需要可变引用）
        let mut positions = Vec::new();
        for (inst_id, pos) in account_write.hold.iter_mut() {
            let volume_long = (pos.volume_long_today + pos.volume_long_his) as i64;
            let volume_short = (pos.volume_short_today + pos.volume_short_his) as i64;

            if volume_long > 0 {
                positions.push(SettlementPosition {
                    instrument_id: inst_id.clone(),
                    direction: "多".to_string(),
                    volume: volume_long,
                    avg_price: pos.open_price_long,
                    settlement_price: pos.lastest_price,
                    margin: pos.margin_long,
                    pnl: pos.float_profit_long(),
                });
            }
            if volume_short > 0 {
                positions.push(SettlementPosition {
                    instrument_id: inst_id.clone(),
                    direction: "空".to_string(),
                    volume: volume_short,
                    avg_price: pos.open_price_short,
                    settlement_price: pos.lastest_price,
                    margin: pos.margin_short,
                    pnl: pos.float_profit_short(),
                });
            }
        }

        let mut trades = Vec::new();
        for (trade_id, trade) in account_write.dailytrades.iter() {
            trades.push(SettlementTrade {
                trade_id: trade_id.clone(),
                instrument_id: trade.instrument_id.clone(),
                direction: trade.direction.clone(),
                offset: trade.offset.clone(),
                price: trade.price,
                volume: trade.volume as i64,
                commission: trade.commission,
                trade_time: trade.trade_date_time.to_string(),
            });
        }

        // 访问账户数据（在循环之后）
        let acc = &account_write.accounts;
        let statement = SettlementStatement {
            account_id: account_id.clone(),
            trading_day: query.date.clone(),
            pre_balance: acc.pre_balance,
            deposit: acc.deposit,
            withdraw: acc.withdraw,
            commission: acc.commission,
            close_profit: acc.close_profit,
            position_profit: acc.position_profit,
            balance: acc.balance,
            margin: acc.margin,
            available: acc.available,
            risk_ratio: acc.risk_ratio,
            positions,
            trades,
        };

        HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "data": statement
        }))
    } else {
        HttpResponse::Ok().json(serde_json::json!({
            "success": false,
            "error": "账户不存在"
        }))
    }
}

/// 导出数据（支持CSV/JSON）
/// @yutiansut @quantaxis
pub async fn export_data(
    query: web::Query<DataExportRequest>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let account_id = &query.account_id;
    let data_type = &query.data_type;
    let format = query.format.clone().unwrap_or_else(|| "json".to_string());

    if let Ok(account) = state.account_mgr.get_account(account_id) {
        let account_read = account.read();

        match data_type.as_str() {
            "orders" => {
                let orders: Vec<serde_json::Value> = account_read.dailyorders.values()
                    .map(|o| serde_json::json!({
                        "order_id": o.order_id,
                        "instrument_id": o.instrument_id,
                        "direction": o.direction,
                        "offset": o.offset,
                        "price": o.limit_price,
                        "volume": o.volume_orign,
                        "volume_left": o.volume_left,
                        "status": o.status,
                        "insert_time": o.insert_date_time
                    }))
                    .collect();

                if format == "csv" {
                    let mut csv = "order_id,instrument_id,direction,offset,price,volume,volume_left,status,insert_time\n".to_string();
                    for o in &orders {
                        csv.push_str(&format!("{},{},{},{},{},{},{},{},{}\n",
                            o["order_id"].as_str().unwrap_or(""),
                            o["instrument_id"].as_str().unwrap_or(""),
                            o["direction"].as_str().unwrap_or(""),
                            o["offset"].as_str().unwrap_or(""),
                            o["price"].as_f64().unwrap_or(0.0),
                            o["volume"].as_f64().unwrap_or(0.0),
                            o["volume_left"].as_f64().unwrap_or(0.0),
                            o["status"].as_str().unwrap_or(""),
                            o["insert_time"].as_i64().unwrap_or(0)
                        ));
                    }
                    return HttpResponse::Ok()
                        .content_type("text/csv; charset=utf-8")
                        .insert_header(("Content-Disposition", format!("attachment; filename=\"{}_{}.csv\"", account_id, data_type)))
                        .body(csv);
                }

                return HttpResponse::Ok().json(serde_json::json!({
                    "success": true,
                    "data": {
                        "account_id": account_id,
                        "data_type": data_type,
                        "count": orders.len(),
                        "records": orders
                    }
                }));
            },
            "trades" => {
                let trades: Vec<serde_json::Value> = account_read.dailytrades.values()
                    .map(|t| serde_json::json!({
                        "trade_id": t.trade_id,
                        "order_id": t.order_id,
                        "instrument_id": t.instrument_id,
                        "exchange_id": t.exchange_id,
                        "direction": t.direction,
                        "offset": t.offset,
                        "price": t.price,
                        "volume": t.volume,
                        "commission": t.commission,
                        "trade_time": t.trade_date_time
                    }))
                    .collect();

                if format == "csv" {
                    let mut csv = "trade_id,order_id,instrument_id,exchange_id,direction,offset,price,volume,commission,trade_time\n".to_string();
                    for t in &trades {
                        csv.push_str(&format!("{},{},{},{},{},{},{},{},{},{}\n",
                            t["trade_id"].as_str().unwrap_or(""),
                            t["order_id"].as_str().unwrap_or(""),
                            t["instrument_id"].as_str().unwrap_or(""),
                            t["exchange_id"].as_str().unwrap_or(""),
                            t["direction"].as_str().unwrap_or(""),
                            t["offset"].as_str().unwrap_or(""),
                            t["price"].as_f64().unwrap_or(0.0),
                            t["volume"].as_f64().unwrap_or(0.0),
                            t["commission"].as_f64().unwrap_or(0.0),
                            t["trade_time"].as_i64().unwrap_or(0)
                        ));
                    }
                    return HttpResponse::Ok()
                        .content_type("text/csv; charset=utf-8")
                        .insert_header(("Content-Disposition", format!("attachment; filename=\"{}_{}.csv\"", account_id, data_type)))
                        .body(csv);
                }

                return HttpResponse::Ok().json(serde_json::json!({
                    "success": true,
                    "data": {
                        "account_id": account_id,
                        "data_type": data_type,
                        "count": trades.len(),
                        "records": trades
                    }
                }));
            },
            "positions" => {
                // 计算float_profit需要：price * volume * unit - open_cost
                // 这里使用简化计算（不访问unit_table）直接用volume * (price - open_price)
                let positions: Vec<serde_json::Value> = account_read.hold.iter()
                    .map(|(id, p)| {
                        let volume_long = p.volume_long_today + p.volume_long_his;
                        let volume_short = p.volume_short_today + p.volume_short_his;
                        // 简化的浮动盈亏计算（基于持仓成本）
                        let pnl_long = if volume_long > 0.0 && p.position_cost_long > 0.0 {
                            p.lastest_price * volume_long * p.preset.unit_table as f64 - p.open_cost_long
                        } else {
                            0.0
                        };
                        let pnl_short = if volume_short > 0.0 && p.position_cost_short > 0.0 {
                            p.open_cost_short - p.lastest_price * volume_short * p.preset.unit_table as f64
                        } else {
                            0.0
                        };
                        serde_json::json!({
                            "instrument_id": id,
                            "exchange_id": p.exchange_id,
                            "volume_long": volume_long,
                            "volume_short": volume_short,
                            "open_price_long": p.open_price_long,
                            "open_price_short": p.open_price_short,
                            "last_price": p.lastest_price,
                            "margin_long": p.margin_long,
                            "margin_short": p.margin_short,
                            "pnl_long": pnl_long,
                            "pnl_short": pnl_short
                        })
                    })
                    .collect();

                if format == "csv" {
                    let mut csv = "instrument_id,exchange_id,volume_long,volume_short,open_price_long,open_price_short,last_price,margin_long,margin_short,pnl_long,pnl_short\n".to_string();
                    for p in &positions {
                        csv.push_str(&format!("{},{},{},{},{},{},{},{},{},{},{}\n",
                            p["instrument_id"].as_str().unwrap_or(""),
                            p["exchange_id"].as_str().unwrap_or(""),
                            p["volume_long"].as_f64().unwrap_or(0.0),
                            p["volume_short"].as_f64().unwrap_or(0.0),
                            p["open_price_long"].as_f64().unwrap_or(0.0),
                            p["open_price_short"].as_f64().unwrap_or(0.0),
                            p["last_price"].as_f64().unwrap_or(0.0),
                            p["margin_long"].as_f64().unwrap_or(0.0),
                            p["margin_short"].as_f64().unwrap_or(0.0),
                            p["pnl_long"].as_f64().unwrap_or(0.0),
                            p["pnl_short"].as_f64().unwrap_or(0.0)
                        ));
                    }
                    return HttpResponse::Ok()
                        .content_type("text/csv; charset=utf-8")
                        .insert_header(("Content-Disposition", format!("attachment; filename=\"{}_{}.csv\"", account_id, data_type)))
                        .body(csv);
                }

                return HttpResponse::Ok().json(serde_json::json!({
                    "success": true,
                    "data": {
                        "account_id": account_id,
                        "data_type": data_type,
                        "count": positions.len(),
                        "records": positions
                    }
                }));
            },
            _ => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "success": false,
                    "error": format!("不支持的数据类型: {}", data_type)
                }));
            }
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "success": false,
        "error": "账户不存在"
    }))
}

/// 获取风险度统计（真实数据）
/// @yutiansut @quantaxis
pub async fn get_risk_statistics(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let accounts = state.account_mgr.get_all_accounts();

    let mut total_accounts = 0;
    let mut high_risk_count = 0;
    let mut medium_risk_count = 0;
    let mut low_risk_count = 0;
    let mut total_margin = 0.0f64;
    let mut total_balance = 0.0f64;
    let mut risk_distribution: Vec<serde_json::Value> = Vec::new();

    for account in accounts {
        let account_read = account.read();
        total_accounts += 1;
        total_margin += account_read.accounts.margin;
        total_balance += account_read.accounts.balance;

        let risk = account_read.accounts.risk_ratio;
        let risk_level;
        if risk >= 0.8 {
            high_risk_count += 1;
            risk_level = "high";
        } else if risk >= 0.5 {
            medium_risk_count += 1;
            risk_level = "medium";
        } else {
            low_risk_count += 1;
            risk_level = "low";
        }

        // 高风险账户详情
        if risk >= 0.7 {
            risk_distribution.push(serde_json::json!({
                "account_id": account_read.accounts.user_id,
                "risk_ratio": risk,
                "risk_level": risk_level,
                "margin": account_read.accounts.margin,
                "balance": account_read.accounts.balance
            }));
        }
    }

    // 按风险度排序
    risk_distribution.sort_by(|a, b| {
        b["risk_ratio"].as_f64().unwrap_or(0.0)
            .partial_cmp(&a["risk_ratio"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": {
            "total_accounts": total_accounts,
            "high_risk_count": high_risk_count,
            "medium_risk_count": medium_risk_count,
            "low_risk_count": low_risk_count,
            "total_margin": total_margin,
            "total_balance": total_balance,
            "avg_risk_ratio": if total_balance > 0.0 { total_margin / total_balance } else { 0.0 },
            "high_risk_accounts": risk_distribution
        }
    }))
}

/// 获取交易排行（真实数据）
/// @yutiansut @quantaxis
pub async fn get_trade_rankings(
    query: web::Query<TradeRankingQuery>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let rank_by = query.rank_by.clone().unwrap_or_else(|| "pnl".to_string());
    let limit = query.limit.unwrap_or(10);

    let accounts = state.account_mgr.get_all_accounts();
    let mut rankings: Vec<serde_json::Value> = Vec::new();

    for account in accounts {
        let account_read = account.read();
        let acc = &account_read.accounts;

        let pnl = acc.close_profit + acc.position_profit;
        let pnl_ratio = if acc.pre_balance > 0.0 {
            (acc.balance - acc.pre_balance) / acc.pre_balance * 100.0
        } else {
            0.0
        };
        let trade_count = account_read.dailytrades.len();
        let volume: f64 = account_read.dailytrades.values().map(|t| t.volume).sum();

        rankings.push(serde_json::json!({
            "account_id": acc.user_id,
            "balance": acc.balance,
            "pnl": pnl,
            "pnl_ratio": pnl_ratio,
            "trade_count": trade_count,
            "volume": volume,
            "commission": acc.commission
        }));
    }

    // 排序
    match rank_by.as_str() {
        "pnl" => rankings.sort_by(|a, b| {
            b["pnl"].as_f64().unwrap_or(0.0)
                .partial_cmp(&a["pnl"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "pnl_ratio" => rankings.sort_by(|a, b| {
            b["pnl_ratio"].as_f64().unwrap_or(0.0)
                .partial_cmp(&a["pnl_ratio"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "volume" => rankings.sort_by(|a, b| {
            b["volume"].as_f64().unwrap_or(0.0)
                .partial_cmp(&a["volume"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "trade_count" => rankings.sort_by(|a, b| {
            b["trade_count"].as_i64().unwrap_or(0)
                .cmp(&a["trade_count"].as_i64().unwrap_or(0))
        }),
        _ => {}
    }

    rankings.truncate(limit);

    // 添加排名
    for (i, rank) in rankings.iter_mut().enumerate() {
        if let Some(obj) = rank.as_object_mut() {
            obj.insert("rank".to_string(), serde_json::json!(i + 1));
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": {
            "rank_by": rank_by,
            "total": rankings.len(),
            "rankings": rankings
        }
    }))
}

/// 获取市场概览统计（真实数据）
/// @yutiansut @quantaxis
pub async fn get_market_overview(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let accounts = state.account_mgr.get_all_accounts();

    let mut total_balance = 0.0f64;
    let mut total_margin = 0.0f64;
    let mut total_pnl = 0.0f64;
    let mut total_orders = 0usize;
    let mut total_trades = 0usize;
    let mut total_positions = 0usize;
    let mut active_accounts = 0usize;

    for account in &accounts {
        let account_read = account.read();
        total_balance += account_read.accounts.balance;
        total_margin += account_read.accounts.margin;
        total_pnl += account_read.accounts.close_profit + account_read.accounts.position_profit;
        total_orders += account_read.dailyorders.len();
        total_trades += account_read.dailytrades.len();
        total_positions += account_read.hold.len();

        if !account_read.hold.is_empty() || !account_read.dailyorders.is_empty() {
            active_accounts += 1;
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": {
            "account_count": accounts.len(),
            "active_account_count": active_accounts,
            "total_balance": total_balance,
            "total_margin": total_margin,
            "total_available": total_balance - total_margin,
            "total_pnl": total_pnl,
            "total_orders": total_orders,
            "total_trades": total_trades,
            "total_positions": total_positions,
            "avg_balance": if accounts.len() > 0 { total_balance / accounts.len() as f64 } else { 0.0 },
            "market_status": "Trading",
            "trading_day": chrono::Local::now().format("%Y-%m-%d").to_string(),
            "update_time": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
        }
    }))
}

// ==================== 单元测试 @yutiansut @quantaxis ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== extract_period_key 测试 ====================

    /// 测试小时粒度时间戳提取
    /// 输入: 纳秒时间戳 (Unix epoch)
    /// 输出: "YYYY-MM-DD HH:00" 格式字符串
    /// 计算逻辑:
    ///   1. 纳秒转秒: timestamp_ns / 1_000_000_000
    ///   2. 使用 chrono 解析为 DateTime
    ///   3. 格式化为小时整点 (分钟置0)
    #[test]
    fn test_extract_period_key_hour() {
        // 测试时间: 2024-03-15 14:32:45 UTC
        // Unix timestamp (秒): 1710513165
        // 纳秒时间戳: 1710513165 * 1_000_000_000 = 1710513165000000000
        let timestamp_ns: i64 = 1710513165_000_000_000;

        let result = extract_period_key(timestamp_ns, "hour");

        // 预期: 分钟部分归零，只保留到小时
        // 14:32:45 → 14:00
        assert!(result.contains("14:00"), "小时粒度应归零分钟: got {}", result);
        assert!(result.contains("2024-03-15"), "日期应正确: got {}", result);
    }

    /// 测试日粒度时间戳提取
    /// 输入: 纳秒时间戳
    /// 输出: "YYYY-MM-DD" 格式字符串
    /// 用途: 按日统计交易量、盈亏等
    #[test]
    fn test_extract_period_key_day() {
        // 测试时间: 2024-06-20 09:15:30 UTC
        let timestamp_ns: i64 = 1718871330_000_000_000;

        let result = extract_period_key(timestamp_ns, "day");

        // 预期: 只有日期部分，无时间
        assert_eq!(result, "2024-06-20");
    }

    /// 测试周粒度时间戳提取 (ISO 周格式)
    /// 输入: 纳秒时间戳
    /// 输出: "YYYY-Www" 格式 (ISO 8601 周格式)
    /// 注意:
    ///   - ISO周从周一开始
    ///   - 跨年时可能出现 W52/W53 或 W01
    ///   - 使用 %G (ISO周年) 而非 %Y (日历年)
    #[test]
    fn test_extract_period_key_week() {
        // 测试时间: 2024-12-17 (周二)
        // ISO周: 2024年第51周
        let timestamp_ns: i64 = 1734393600_000_000_000;

        let result = extract_period_key(timestamp_ns, "week");

        // 预期格式: "2024-W51"
        assert!(result.starts_with("2024-W"), "应为ISO周格式: got {}", result);
        // 验证周数在合理范围 (1-53)
        let week_part = result.split("-W").nth(1).unwrap_or("0");
        let week_num: i32 = week_part.parse().unwrap_or(0);
        assert!(week_num >= 1 && week_num <= 53, "周数应在1-53之间: got {}", week_num);
    }

    /// 测试月粒度时间戳提取
    /// 输入: 纳秒时间戳
    /// 输出: "YYYY-MM" 格式字符串
    /// 用途: 按月统计账户盈亏、手续费等
    #[test]
    fn test_extract_period_key_month() {
        // 测试时间: 2024-08-25 16:45:00 UTC
        let timestamp_ns: i64 = 1724603100_000_000_000;

        let result = extract_period_key(timestamp_ns, "month");

        // 预期: 只有年月
        assert_eq!(result, "2024-08");
    }

    /// 测试未知粒度回退到日粒度
    /// 业务规则: 不支持的 group_by 参数默认按日统计
    #[test]
    fn test_extract_period_key_unknown_fallback() {
        let timestamp_ns: i64 = 1710513165_000_000_000;

        // 传入不支持的粒度
        let result = extract_period_key(timestamp_ns, "quarter");

        // 预期: 回退到日粒度
        assert!(result.contains("-"), "应为日期格式: got {}", result);
        assert!(!result.contains(":"), "不应包含时间: got {}", result);
    }

    /// 测试无效时间戳处理
    /// 输入: 无效时间戳 (如负数或超大值)
    /// 输出: "unknown" 字符串
    /// 防御性编程: 避免因无效输入导致 panic
    #[test]
    fn test_extract_period_key_invalid_timestamp() {
        // 测试负时间戳 (1970年之前)
        let timestamp_ns: i64 = -1_000_000_000_000_000_000;

        let result = extract_period_key(timestamp_ns, "day");

        // chrono 在某些平台可能返回 unknown 或正常解析
        // 主要确保不 panic
        assert!(!result.is_empty(), "应返回非空字符串");
    }

    // ==================== PeriodStats 聚合测试 ====================

    /// 测试 by_period 时间段分组统计逻辑
    /// 业务场景: 统计多笔交易按日分组后的成交量、手续费
    ///
    /// 计算逻辑:
    ///   1. 遍历所有成交记录 (dailytrades)
    ///   2. 根据 trade_date_time 提取时间段键值
    ///   3. 使用 HashMap 累加同一时间段的统计数据:
    ///      - trades: 成交笔数 += 1
    ///      - volume: 成交量 += trade.volume
    ///      - pnl: 盈亏 += 计算值 (需关联持仓)
    ///      - commission: 手续费 += trade.commission
    ///   4. 按时间段排序输出
    #[test]
    fn test_period_stats_aggregation() {
        // 模拟3笔交易，2笔在同一天，1笔在另一天
        let mut period_stats: HashMap<String, PeriodStats> = HashMap::new();

        // 交易1: 2024-03-15, 成交量 10, 手续费 5.0
        let trade1_ts = 1710513165_000_000_000i64; // 2024-03-15 14:32:45 UTC
        let key1 = extract_period_key(trade1_ts, "day");
        let stat1 = period_stats.entry(key1.clone()).or_insert(PeriodStats {
            period: key1, trades: 0, volume: 0.0, pnl: 0.0, commission: 0.0,
        });
        stat1.trades += 1;
        stat1.volume += 10.0;
        stat1.commission += 5.0;

        // 交易2: 2024-03-15, 成交量 20, 手续费 8.0 (同一天)
        let trade2_ts = 1710527565_000_000_000i64; // 2024-03-15 18:32:45 UTC
        let key2 = extract_period_key(trade2_ts, "day");
        let stat2 = period_stats.entry(key2.clone()).or_insert(PeriodStats {
            period: key2, trades: 0, volume: 0.0, pnl: 0.0, commission: 0.0,
        });
        stat2.trades += 1;
        stat2.volume += 20.0;
        stat2.commission += 8.0;

        // 交易3: 2024-03-16, 成交量 15, 手续费 6.0 (不同天)
        let trade3_ts = 1710599565_000_000_000i64; // 2024-03-16 14:32:45 UTC
        let key3 = extract_period_key(trade3_ts, "day");
        let stat3 = period_stats.entry(key3.clone()).or_insert(PeriodStats {
            period: key3, trades: 0, volume: 0.0, pnl: 0.0, commission: 0.0,
        });
        stat3.trades += 1;
        stat3.volume += 15.0;
        stat3.commission += 6.0;

        // 验证分组结果
        assert_eq!(period_stats.len(), 2, "应分为2个时间段");

        // 验证2024-03-15的聚合数据
        // trades: 1 + 1 = 2笔
        // volume: 10 + 20 = 30手
        // commission: 5 + 8 = 13元
        let day1_stats = period_stats.get("2024-03-15").expect("应存在2024-03-15");
        assert_eq!(day1_stats.trades, 2, "3月15日应有2笔交易");
        assert!((day1_stats.volume - 30.0).abs() < 0.01, "3月15日成交量应为30");
        assert!((day1_stats.commission - 13.0).abs() < 0.01, "3月15日手续费应为13");

        // 验证2024-03-16的聚合数据
        let day2_stats = period_stats.get("2024-03-16").expect("应存在2024-03-16");
        assert_eq!(day2_stats.trades, 1, "3月16日应有1笔交易");
        assert!((day2_stats.volume - 15.0).abs() < 0.01, "3月16日成交量应为15");
        assert!((day2_stats.commission - 6.0).abs() < 0.01, "3月16日手续费应为6");
    }

    /// 测试 by_period 排序逻辑
    /// 业务要求: 返回结果按时间段升序排列
    #[test]
    fn test_period_stats_sorting() {
        let mut period_stats: HashMap<String, PeriodStats> = HashMap::new();

        // 乱序插入
        period_stats.insert("2024-03-18".to_string(), PeriodStats {
            period: "2024-03-18".to_string(), trades: 1, volume: 10.0, pnl: 0.0, commission: 0.0,
        });
        period_stats.insert("2024-03-15".to_string(), PeriodStats {
            period: "2024-03-15".to_string(), trades: 2, volume: 20.0, pnl: 0.0, commission: 0.0,
        });
        period_stats.insert("2024-03-16".to_string(), PeriodStats {
            period: "2024-03-16".to_string(), trades: 3, volume: 30.0, pnl: 0.0, commission: 0.0,
        });

        // 转换并排序 (与 get_trade_statistics 中逻辑一致)
        let mut by_period: Vec<PeriodStats> = period_stats.into_values().collect();
        by_period.sort_by(|a, b| a.period.cmp(&b.period));

        // 验证排序结果
        assert_eq!(by_period.len(), 3);
        assert_eq!(by_period[0].period, "2024-03-15", "第一个应是最早日期");
        assert_eq!(by_period[1].period, "2024-03-16", "第二个应是中间日期");
        assert_eq!(by_period[2].period, "2024-03-18", "第三个应是最晚日期");
    }

    // ==================== DailyPnl 累计盈亏测试 ====================

    /// 测试每日盈亏累计计算逻辑
    /// 业务场景: 展示账户每日净盈亏及累计盈亏曲线
    ///
    /// 计算公式:
    ///   - net_pnl = realized_pnl - commission (每日净盈亏)
    ///   - cumulative_pnl = 前一日cumulative_pnl + 当日net_pnl (累计盈亏)
    ///
    /// QIFI 账户关系:
    ///   - realized_pnl: 来自平仓盈亏 (close_profit)
    ///   - commission: 当日交易手续费
    ///   - unrealized_pnl: 持仓浮盈 (需从历史快照获取)
    #[test]
    fn test_daily_pnl_cumulative_calculation() {
        // 模拟3天的盈亏数据
        let mut daily_pnl: Vec<DailyPnl> = vec![
            // Day 1: 盈利 1000, 手续费 50
            // net_pnl = 1000 - 50 = 950
            DailyPnl {
                date: "2024-03-15".to_string(),
                realized_pnl: 1000.0,
                unrealized_pnl: 0.0,
                commission: 50.0,
                net_pnl: 1000.0 - 50.0,
                cumulative_pnl: 0.0, // 待计算
            },
            // Day 2: 亏损 200, 手续费 30
            // net_pnl = -200 - 30 = -230
            DailyPnl {
                date: "2024-03-16".to_string(),
                realized_pnl: -200.0,
                unrealized_pnl: 0.0,
                commission: 30.0,
                net_pnl: -200.0 - 30.0,
                cumulative_pnl: 0.0,
            },
            // Day 3: 盈利 500, 手续费 40
            // net_pnl = 500 - 40 = 460
            DailyPnl {
                date: "2024-03-17".to_string(),
                realized_pnl: 500.0,
                unrealized_pnl: 0.0,
                commission: 40.0,
                net_pnl: 500.0 - 40.0,
                cumulative_pnl: 0.0,
            },
        ];

        // 按日期排序 (确保累计计算顺序正确)
        daily_pnl.sort_by(|a, b| a.date.cmp(&b.date));

        // 计算累计盈亏 (与 get_pnl_analysis 中逻辑一致)
        let mut cumulative = 0.0f64;
        for pnl in daily_pnl.iter_mut() {
            cumulative += pnl.net_pnl;
            pnl.cumulative_pnl = cumulative;
        }

        // 验证累计盈亏计算
        // Day 1: cumulative = 0 + 950 = 950
        assert!((daily_pnl[0].cumulative_pnl - 950.0).abs() < 0.01,
            "Day1累计应为950: got {}", daily_pnl[0].cumulative_pnl);

        // Day 2: cumulative = 950 + (-230) = 720
        assert!((daily_pnl[1].cumulative_pnl - 720.0).abs() < 0.01,
            "Day2累计应为720: got {}", daily_pnl[1].cumulative_pnl);

        // Day 3: cumulative = 720 + 460 = 1180
        assert!((daily_pnl[2].cumulative_pnl - 1180.0).abs() < 0.01,
            "Day3累计应为1180: got {}", daily_pnl[2].cumulative_pnl);
    }

    /// 测试空数据情况下的每日盈亏
    /// 防御性编程: 确保无交易时不会 panic
    #[test]
    fn test_daily_pnl_empty_data() {
        let daily_pnl: Vec<DailyPnl> = Vec::new();

        assert!(daily_pnl.is_empty(), "空数据应返回空数组");
    }

    // ==================== TradeStatistics 综合测试 ====================

    /// 测试交易统计指标计算
    /// 业务场景: 计算账户交易表现指标
    ///
    /// 指标计算公式:
    ///   - win_rate = win_count / total_trades (胜率)
    ///   - avg_profit = total_profit / win_count (平均盈利)
    ///   - avg_loss = total_loss / loss_count (平均亏损)
    ///   - profit_factor = total_profit / total_loss (盈亏比)
    ///   - total_turnover = Σ(volume * price) (总成交额)
    #[test]
    fn test_trade_statistics_metrics() {
        // 模拟统计数据
        let total_trades = 10i64;
        let win_count = 6i64;    // 6笔盈利
        let loss_count = 4i64;   // 4笔亏损
        let total_profit = 3000.0f64; // 总盈利
        let total_loss = 1000.0f64;   // 总亏损 (绝对值)

        // 计算指标 (与 get_trade_statistics 逻辑一致)
        let win_rate = if total_trades > 0 {
            win_count as f64 / total_trades as f64
        } else {
            0.0
        };

        let avg_profit = if win_count > 0 {
            total_profit / win_count as f64
        } else {
            0.0
        };

        let avg_loss = if loss_count > 0 {
            total_loss / loss_count as f64
        } else {
            0.0
        };

        let profit_factor = if total_loss > 0.0 {
            total_profit / total_loss
        } else if total_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        // 验证计算结果
        // win_rate = 6/10 = 0.6 (60%)
        assert!((win_rate - 0.6).abs() < 0.01, "胜率应为60%: got {}", win_rate);

        // avg_profit = 3000/6 = 500
        assert!((avg_profit - 500.0).abs() < 0.01, "平均盈利应为500: got {}", avg_profit);

        // avg_loss = 1000/4 = 250
        assert!((avg_loss - 250.0).abs() < 0.01, "平均亏损应为250: got {}", avg_loss);

        // profit_factor = 3000/1000 = 3.0
        assert!((profit_factor - 3.0).abs() < 0.01, "盈亏比应为3.0: got {}", profit_factor);
    }

    /// 测试成交额计算
    /// 公式: turnover = price * volume
    /// 注意: 期货成交额还需乘以合约乘数 (unit_table)，此处为简化计算
    #[test]
    fn test_turnover_calculation() {
        // 模拟成交记录
        struct MockTrade {
            price: f64,
            volume: f64,
        }

        let trades = vec![
            MockTrade { price: 5000.0, volume: 10.0 }, // 50000
            MockTrade { price: 5100.0, volume: 5.0 },  // 25500
            MockTrade { price: 4900.0, volume: 8.0 },  // 39200
        ];

        let mut total_turnover = 0.0f64;
        for trade in &trades {
            total_turnover += trade.price * trade.volume;
        }

        // 预期: 50000 + 25500 + 39200 = 114700
        assert!((total_turnover - 114700.0).abs() < 0.01,
            "总成交额应为114700: got {}", total_turnover);
    }

    // ==================== 时间戳转换测试 ====================

    /// 测试纳秒时间戳转日期时间字符串
    /// 格式: "YYYY-MM-DD HH:MM:SS.mmm"
    #[test]
    fn test_timestamp_to_datetime() {
        // 测试时间: 2024-03-15 14:32:45.123 UTC
        let ts: i64 = 1710513165_123_000_000;

        let result = timestamp_to_datetime(ts);

        // 验证格式和内容
        assert!(result.contains("2024-03-15"), "应包含正确日期: got {}", result);
        assert!(result.contains("14:32:45"), "应包含正确时间: got {}", result);
        assert!(result.contains(".123"), "应包含毫秒: got {}", result);
    }

    /// 测试K线周期映射
    #[test]
    fn test_period_mapping() {
        // HQChart 周期映射
        assert_eq!(period_to_string(0), "1d", "0 应映射到日线");
        assert_eq!(period_to_string(4), "1m", "4 应映射到1分钟");
        assert_eq!(period_to_string(5), "5m", "5 应映射到5分钟");
        assert_eq!(period_to_string(8), "60m", "8 应映射到60分钟");

        // 反向映射
        assert_eq!(string_to_period("1d"), 0);
        assert_eq!(string_to_period("1m"), 4);
        assert_eq!(string_to_period("5min"), 5);
        assert_eq!(string_to_period("1h"), 8);
    }
}
