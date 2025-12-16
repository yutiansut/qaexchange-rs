//! DIFF 协议消息定义
//!
//! DIFF (Differential Information Flow for Finance) 协议消息类型
//!
//! # 消息类型
//!
//! - 客户端: `DiffClientMessage` (aid-based)
//! - 服务端: `DiffServerMessage` (aid-based)
//!
//! # 与现有消息的关系
//!
//! DIFF 消息与现有的 `ClientMessage` 和 `ServerMessage` 独立，
//! 通过不同的 tag 字段区分（aid vs type）

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// DIFF 协议客户端消息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "aid", rename_all = "snake_case")]
pub enum DiffClientMessage {
    /// 心跳请求 @yutiansut @quantaxis
    Ping,

    /// 业务信息截面更新请求（peek_message）
    PeekMessage,

    /// 登录请求
    ReqLogin {
        #[serde(skip_serializing_if = "Option::is_none")]
        bid: Option<String>,
        user_name: String,
        password: String,
    },

    /// 订阅行情
    SubscribeQuote {
        ins_list: String, // 逗号分隔的合约列表，如 "SHFE.cu1612,CFFEX.IF1701"
    },

    /// 下单
    InsertOrder {
        user_id: String, // 用户身份（用于验证）
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        account_id: Option<String>, // 交易账户（推荐明确传递）✨
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        order_id: Option<String>, // 订单ID（可选，未提供时服务端自动生成）
        exchange_id: String,
        instrument_id: String,
        direction: String, // BUY/SELL
        offset: String,    // OPEN/CLOSE
        volume: i64,
        price_type: String, // LIMIT/MARKET/ANY
        #[serde(skip_serializing_if = "Option::is_none")]
        limit_price: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        volume_condition: Option<String>, // ANY/MIN/ALL
        #[serde(skip_serializing_if = "Option::is_none")]
        time_condition: Option<String>, // IOC/GFS/GFD/GTD/GTC/GFA
    },

    /// 撤单
    CancelOrder {
        user_id: String, // 用户身份（用于验证）
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        account_id: Option<String>, // 交易账户（推荐明确传递）✨
        order_id: String,
    },

    /// 订阅图表数据
    SetChart {
        chart_id: String,
        ins_list: String, // 空表示删除，多个合约逗号分隔
        duration: i64,    // 周期(ns)，tick=0, 日线=86400000000000
        view_width: i32,  // 图表宽度
    },
}

/// DIFF 协议服务端消息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "aid", rename_all = "snake_case")]
pub enum DiffServerMessage {
    /// 心跳响应 @yutiansut @quantaxis
    Pong,

    /// 业务信息截面更新（rtn_data）
    RtnData {
        data: Vec<Value>, // JSON Merge Patch 数组
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ═══════════════════════════════════════════════════════════════════════════
    // 基础消息序列化测试
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_peek_message_serialization() {
        // -------------------------------------------------------------------------
        // 测试内容: peek_message 消息的序列化
        // -------------------------------------------------------------------------
        // DIFF 协议中，客户端发送 peek_message 请求服务端推送最新的业务截面数据
        // 服务端收到后检查是否有更新，有则立即发送 rtn_data，无则等待更新后再发送
        //
        // 消息格式:
        //   客户端 -> 服务端: {"aid": "peek_message"}
        //   服务端 -> 客户端: {"aid": "rtn_data", "data": [...]}
        // -------------------------------------------------------------------------
        let msg = DiffClientMessage::PeekMessage;
        let json = serde_json::to_value(&msg).unwrap();

        // 验证 aid 字段正确序列化为 "peek_message"
        assert_eq!(json["aid"], "peek_message");

        // 验证反序列化
        let parsed: DiffClientMessage =
            serde_json::from_str(r#"{"aid":"peek_message"}"#).unwrap();
        assert!(matches!(parsed, DiffClientMessage::PeekMessage));
    }

    #[test]
    fn test_rtn_data_serialization() {
        // -------------------------------------------------------------------------
        // 测试内容: rtn_data 消息的序列化（业务截面更新）
        // -------------------------------------------------------------------------
        // rtn_data 是 DIFF 协议的核心消息，用于推送业务截面的差分更新
        // data 数组中每个元素都是一个 JSON Merge Patch (RFC 7386)
        //
        // 数据处理规则:
        // 1. 整个 data 数组视为一个事务，必须全部处理完才能提取数据
        // 2. 处理过程中业务截面可能处于内部不一致状态
        // 3. 没有变化的字段服务端也可能发送（用于确认状态）
        //
        // 账户数据更新示例:
        //   balance = static_balance + float_profit (QIFI 自恰性规则)
        // -------------------------------------------------------------------------
        let msg = DiffServerMessage::RtnData {
            data: vec![
                // 第一个 patch: 更新账户余额
                json!({
                    "balance": 100000.0,
                    "available": 80000.0,
                    "margin": 20000.0
                }),
                // 第二个 patch: 更新持仓浮盈
                json!({
                    "float_profit": 1500.0,
                    "position_profit": 1200.0
                }),
            ],
        };
        let json_str = serde_json::to_string(&msg).unwrap();

        // 验证 aid 字段
        assert!(json_str.contains("\"aid\":\"rtn_data\""));

        // 验证 data 数组包含两个 patch
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["data"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_rtn_data_account_update() {
        // -------------------------------------------------------------------------
        // 测试内容: rtn_data 推送账户数据更新
        // -------------------------------------------------------------------------
        // 模拟真实的 QIFI 账户结构更新场景
        //
        // QIFI 账户计算公式 (accounts 结构):
        //   static_balance = pre_balance + deposit - withdraw + close_profit - commission
        //   float_profit = position_profit (持仓浮盈 = 逐日盯市浮盈)
        //   balance = static_balance + float_profit
        //   available = balance - margin - frozen_margin
        //   risk_ratio = margin / balance (风险度)
        //
        // 示例场景: 账户开仓后的状态变化
        //   初始: pre_balance=100000, deposit=0, withdraw=0
        //   操作: 开多1手 cu2512 @ 75000, 保证金率=10%
        //   结果: margin=7500, available=92500
        // -------------------------------------------------------------------------
        let account_update = json!({
            "trade": {
                "user123": {
                    "user_id": "user123",
                    "accounts": {
                        "CNY": {
                            "user_id": "user123",
                            "currency": "CNY",
                            "pre_balance": 100000.0,      // 昨日结算余额
                            "deposit": 0.0,               // 今日入金
                            "withdraw": 0.0,              // 今日出金
                            "static_balance": 100000.0,   // 静态权益
                            "close_profit": 0.0,          // 平仓盈亏
                            "commission": 12.5,           // 手续费
                            "position_profit": 0.0,       // 持仓盯市盈亏
                            "float_profit": 0.0,          // 浮动盈亏
                            "balance": 99987.5,           // 动态权益 = static - commission
                            "margin": 7500.0,             // 占用保证金
                            "frozen_margin": 0.0,         // 冻结保证金
                            "available": 92487.5,         // 可用资金 = balance - margin
                            "risk_ratio": 0.075           // 风险度 = margin/balance
                        }
                    }
                }
            }
        });

        let msg = DiffServerMessage::RtnData {
            data: vec![account_update],
        };

        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        // 验证账户结构正确序列化
        let accounts = &parsed["data"][0]["trade"]["user123"]["accounts"]["CNY"];
        assert_eq!(accounts["balance"], 99987.5);
        assert_eq!(accounts["margin"], 7500.0);
        assert_eq!(accounts["available"], 92487.5);

        // 验证计算公式: available = balance - margin - frozen_margin
        let balance = accounts["balance"].as_f64().unwrap();
        let margin = accounts["margin"].as_f64().unwrap();
        let frozen_margin = accounts["frozen_margin"].as_f64().unwrap();
        let available = accounts["available"].as_f64().unwrap();
        assert!((available - (balance - margin - frozen_margin)).abs() < 0.01);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // 订单相关消息测试
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_insert_order_serialization() {
        // -------------------------------------------------------------------------
        // 测试内容: insert_order 下单消息序列化
        // -------------------------------------------------------------------------
        // 客户端发送下单请求，包含完整的订单参数
        //
        // 关键字段说明:
        //   user_id: 用户身份标识（用于权限验证）
        //   account_id: 交易账户ID（可选，用于多账户场景）
        //   order_id: 订单ID（可选，未提供时服务端自动生成）
        //   exchange_id: 交易所代码 (SHFE/DCE/CZCE/CFFEX/INE)
        //   instrument_id: 合约代码 (如 cu2512)
        //   direction: 买卖方向 (BUY/SELL)
        //   offset: 开平标志 (OPEN/CLOSE/CLOSETODAY/CLOSEYESTERDAY)
        //   price_type: 价格类型 (LIMIT/MARKET/ANY/BEST/FIVELEVEL)
        //   time_condition: 有效期 (IOC/GFS/GFD/GTD/GTC/GFA)
        //   volume_condition: 成交量条件 (ANY/MIN/ALL)
        // -------------------------------------------------------------------------
        let msg = DiffClientMessage::InsertOrder {
            account_id: Some("account123".to_string()),
            user_id: "user123".to_string(),
            order_id: Some("order1".to_string()),
            exchange_id: "SHFE".to_string(),
            instrument_id: "cu2512".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 10,
            price_type: "LIMIT".to_string(),
            limit_price: Some(75230.0),
            volume_condition: None,
            time_condition: None,
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["aid"], "insert_order");
        assert_eq!(json["user_id"], "user123");
        assert_eq!(json["exchange_id"], "SHFE");
        assert_eq!(json["instrument_id"], "cu2512");
        assert_eq!(json["volume"], 10);
        assert_eq!(json["limit_price"], 75230.0);
    }

    #[test]
    fn test_insert_order_with_time_condition() {
        // -------------------------------------------------------------------------
        // 测试内容: 带有效期条件的下单消息
        // -------------------------------------------------------------------------
        // 测试 IOC (Immediate Or Cancel) 订单
        //
        // IOC 订单特性:
        //   - 立即成交，未成交部分立即撤销
        //   - 常用于需要快速成交的场景
        //   - 不会在订单簿中挂单等待
        //
        // 其他有效期类型:
        //   GFD (Good For Day): 当日有效，收盘自动撤销
        //   GTC (Good Till Cancel): 撤销前有效
        //   GTD (Good Till Date): 指定日期前有效
        //   FOK (Fill Or Kill): 全部成交或全部撤销 (volume_condition=ALL + time_condition=IOC)
        // -------------------------------------------------------------------------
        let msg = DiffClientMessage::InsertOrder {
            account_id: Some("account123".to_string()),
            user_id: "user123".to_string(),
            order_id: None, // 服务端自动生成
            exchange_id: "CFFEX".to_string(),
            instrument_id: "IF2512".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume: 1,
            price_type: "LIMIT".to_string(),
            limit_price: Some(4500.0),
            volume_condition: Some("ANY".to_string()),  // 任意数量
            time_condition: Some("IOC".to_string()),    // 立即成交否则撤销
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["time_condition"], "IOC");
        assert_eq!(json["volume_condition"], "ANY");

        // order_id 为 None 时不应出现在 JSON 中
        assert!(json.get("order_id").is_none() || json["order_id"].is_null());
    }

    #[test]
    fn test_cancel_order_serialization() {
        // -------------------------------------------------------------------------
        // 测试内容: cancel_order 撤单消息序列化
        // -------------------------------------------------------------------------
        // 撤单请求需要指定要撤销的订单ID
        //
        // 撤单流程:
        // 1. 客户端发送 cancel_order
        // 2. 服务端验证订单归属和状态
        // 3. 向交易所发送撤单请求
        // 4. 通过 rtn_data 推送订单状态更新 (status: FINISHED, last_msg: "已撤单")
        //
        // 注意: 已成交的订单无法撤销
        // -------------------------------------------------------------------------
        let msg = DiffClientMessage::CancelOrder {
            user_id: "user123".to_string(),
            account_id: Some("account123".to_string()),
            order_id: "order1".to_string(),
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["aid"], "cancel_order");
        assert_eq!(json["order_id"], "order1");
    }

    #[test]
    fn test_rtn_data_order_status_update() {
        // -------------------------------------------------------------------------
        // 测试内容: rtn_data 推送订单状态更新
        // -------------------------------------------------------------------------
        // 订单状态变化通过 rtn_data 推送给客户端
        //
        // QIFI 订单状态 (status):
        //   ALIVE: 活动中（挂单等待成交）
        //   FINISHED: 已完成（全部成交或已撤销）
        //
        // 订单生命周期示例:
        //   1. 下单成功: status=ALIVE, volume_left=volume_orign
        //   2. 部分成交: status=ALIVE, volume_left减少
        //   3. 全部成交: status=FINISHED, volume_left=0, last_msg="全部成交"
        //   4. 已撤单:   status=FINISHED, volume_left>0, last_msg="已撤单"
        // -------------------------------------------------------------------------
        let order_update = json!({
            "trade": {
                "user123": {
                    "orders": {
                        "order1": {
                            "seqno": 1,
                            "user_id": "user123",
                            "order_id": "order1",
                            "exchange_id": "SHFE",
                            "instrument_id": "cu2512",
                            "direction": "BUY",
                            "offset": "OPEN",
                            "volume_orign": 10.0,         // 原始委托数量
                            "price_type": "LIMIT",
                            "limit_price": 75230.0,
                            "time_condition": "GFD",
                            "volume_condition": "ANY",
                            "insert_date_time": 1734444800000000000_i64,  // 纳秒时间戳
                            "exchange_order_id": "12345678",
                            "status": "ALIVE",            // 订单状态
                            "volume_left": 7.0,           // 剩余数量 (已成交3手)
                            "last_msg": "部分成交"
                        }
                    }
                }
            }
        });

        let msg = DiffServerMessage::RtnData {
            data: vec![order_update],
        };

        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let order = &parsed["data"][0]["trade"]["user123"]["orders"]["order1"];
        assert_eq!(order["status"], "ALIVE");
        assert_eq!(order["volume_orign"], 10.0);
        assert_eq!(order["volume_left"], 7.0);

        // 计算已成交数量
        let filled = order["volume_orign"].as_f64().unwrap() - order["volume_left"].as_f64().unwrap();
        assert_eq!(filled, 3.0);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // 持仓相关消息测试
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_rtn_data_position_update() {
        // -------------------------------------------------------------------------
        // 测试内容: rtn_data 推送持仓数据更新
        // -------------------------------------------------------------------------
        // 持仓数据包含多空双向持仓信息
        //
        // QIFI 持仓计算公式:
        //   volume_long = volume_long_today + volume_long_his (多头总持仓)
        //   volume_short = volume_short_today + volume_short_his (空头总持仓)
        //   float_profit = float_profit_long + float_profit_short (浮动盈亏)
        //   margin = margin_long + margin_short (占用保证金)
        //
        // 浮动盈亏计算:
        //   多头浮盈 = (last_price - open_price_long) * volume_long * multiplier
        //   空头浮盈 = (open_price_short - last_price) * volume_short * multiplier
        //
        // 持仓盯市盈亏计算 (逐日盯市):
        //   多头盯市 = (last_price - position_price_long) * volume_long * multiplier
        //   position_price_long 通常为昨日结算价
        // -------------------------------------------------------------------------
        let position_update = json!({
            "trade": {
                "user123": {
                    "positions": {
                        "SHFE.cu2512": {
                            "user_id": "user123",
                            "exchange_id": "SHFE",
                            "instrument_id": "cu2512",
                            // 多头持仓
                            "volume_long_today": 5.0,      // 今日开多
                            "volume_long_his": 3.0,        // 历史多仓
                            "volume_long": 8.0,            // 多头总持仓
                            "volume_long_frozen_today": 0.0,
                            "volume_long_frozen_his": 0.0,
                            "volume_long_frozen": 0.0,
                            // 空头持仓
                            "volume_short_today": 0.0,
                            "volume_short_his": 0.0,
                            "volume_short": 0.0,
                            "volume_short_frozen_today": 0.0,
                            "volume_short_frozen_his": 0.0,
                            "volume_short_frozen": 0.0,
                            // 持仓均价
                            "open_price_long": 75000.0,    // 开仓均价
                            "open_price_short": 0.0,
                            "open_cost_long": 3000000.0,   // 开仓成本 = 75000 * 8 * 5
                            "open_cost_short": 0.0,
                            "position_price_long": 75200.0, // 持仓均价(逐日盯市)
                            "position_price_short": 0.0,
                            "position_cost_long": 3008000.0, // 持仓成本
                            "position_cost_short": 0.0,
                            // 最新价和盈亏
                            "last_price": 75500.0,
                            "float_profit_long": 20000.0,   // 浮盈 = (75500-75000)*8*5
                            "float_profit_short": 0.0,
                            "float_profit": 20000.0,
                            "position_profit_long": 12000.0, // 盯市盈亏 = (75500-75200)*8*5
                            "position_profit_short": 0.0,
                            "position_profit": 12000.0,
                            // 保证金
                            "margin_long": 30200.0,         // 保证金 = 75500*8*5*10%
                            "margin_short": 0.0,
                            "margin": 30200.0
                        }
                    }
                }
            }
        });

        let msg = DiffServerMessage::RtnData {
            data: vec![position_update],
        };

        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let pos = &parsed["data"][0]["trade"]["user123"]["positions"]["SHFE.cu2512"];

        // 验证持仓数量计算
        let vol_long = pos["volume_long"].as_f64().unwrap();
        let vol_long_today = pos["volume_long_today"].as_f64().unwrap();
        let vol_long_his = pos["volume_long_his"].as_f64().unwrap();
        assert_eq!(vol_long, vol_long_today + vol_long_his);

        // 验证浮动盈亏计算 (铜合约乘数=5)
        let last_price = pos["last_price"].as_f64().unwrap();
        let open_price = pos["open_price_long"].as_f64().unwrap();
        let multiplier = 5.0;
        let expected_float_profit = (last_price - open_price) * vol_long * multiplier;
        let actual_float_profit = pos["float_profit_long"].as_f64().unwrap();
        assert!((expected_float_profit - actual_float_profit).abs() < 0.01);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // 心跳消息测试
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_ping_serialization() {
        // -------------------------------------------------------------------------
        // 测试内容: ping 心跳请求消息序列化
        // -------------------------------------------------------------------------
        // DIFF 协议心跳机制:
        //   客户端定期发送 ping 保持连接活跃
        //   服务端收到 ping 后立即返回 pong
        //
        // 心跳作用:
        // 1. 检测连接是否存活
        // 2. 防止中间网络设备（如NAT）超时断开连接
        // 3. 快速发现网络故障
        //
        // 推荐心跳间隔: 10-30秒
        // 超时判定: 连续3次未收到pong则断开重连
        //
        // 注意: 这是应用层心跳，与 WebSocket 协议层的 Ping/Pong 帧不同
        // -------------------------------------------------------------------------
        let msg = DiffClientMessage::Ping;
        let json = serde_json::to_value(&msg).unwrap();

        // 验证序列化格式
        assert_eq!(json["aid"], "ping");

        // 验证反序列化
        let parsed: DiffClientMessage =
            serde_json::from_str(r#"{"aid":"ping"}"#).unwrap();
        assert!(matches!(parsed, DiffClientMessage::Ping));
    }

    #[test]
    fn test_pong_serialization() {
        // -------------------------------------------------------------------------
        // 测试内容: pong 心跳响应消息序列化
        // -------------------------------------------------------------------------
        // 服务端收到 ping 后立即返回 pong
        //
        // 响应时序:
        //   T0: 客户端发送 {"aid":"ping"}
        //   T1: 服务端收到并处理
        //   T2: 服务端返回 {"aid":"pong"}
        //   T3: 客户端收到 pong，更新心跳时间戳
        //
        // RTT (Round-Trip Time) = T3 - T0
        // 可用于监控网络延迟
        // -------------------------------------------------------------------------
        let msg = DiffServerMessage::Pong;
        let json = serde_json::to_string(&msg).unwrap();

        // 验证包含正确的 aid 字段
        assert!(json.contains("\"aid\":\"pong\""));

        // 验证 JSON 格式完整性
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["aid"], "pong");

        // pong 消息不应包含其他字段
        assert!(value.as_object().unwrap().len() == 1);
    }

    #[test]
    fn test_ping_pong_roundtrip() {
        // -------------------------------------------------------------------------
        // 测试内容: ping/pong 完整往返测试
        // -------------------------------------------------------------------------
        // 模拟完整的心跳交互流程
        //
        // 场景: 客户端发送 ping，验证服务端应返回的 pong 格式
        // -------------------------------------------------------------------------

        // 1. 客户端构造 ping
        let ping = DiffClientMessage::Ping;
        let ping_json = serde_json::to_string(&ping).unwrap();

        // 2. 验证 ping 格式正确
        assert_eq!(ping_json, r#"{"aid":"ping"}"#);

        // 3. 模拟服务端收到 ping 后返回 pong
        let pong = DiffServerMessage::Pong;
        let pong_json = serde_json::to_string(&pong).unwrap();

        // 4. 验证 pong 格式正确
        assert_eq!(pong_json, r#"{"aid":"pong"}"#);

        // 5. 验证客户端能正确解析 pong
        let parsed_pong: serde_json::Value = serde_json::from_str(&pong_json).unwrap();
        assert_eq!(parsed_pong["aid"], "pong");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // 行情订阅测试
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_subscribe_quote_serialization() {
        // -------------------------------------------------------------------------
        // 测试内容: subscribe_quote 行情订阅消息
        // -------------------------------------------------------------------------
        // 客户端发送订阅请求，指定要订阅的合约列表
        //
        // 合约代码格式: 交易所.合约
        //   SHFE.cu2512 - 上期所铜2512
        //   DCE.i2501   - 大商所铁矿石2501
        //   CFFEX.IF2512 - 中金所沪深300指数2512
        //
        // 注意:
        // 1. 合约代码大小写敏感
        // 2. 多次订阅会覆盖之前的订阅列表
        // 3. ins_list 为空表示取消所有订阅
        // -------------------------------------------------------------------------
        let msg = DiffClientMessage::SubscribeQuote {
            ins_list: "SHFE.cu2512,CFFEX.IF2512,DCE.i2501".to_string(),
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["aid"], "subscribe_quote");
        assert_eq!(json["ins_list"], "SHFE.cu2512,CFFEX.IF2512,DCE.i2501");
    }

    #[test]
    fn test_rtn_data_quote_update() {
        // -------------------------------------------------------------------------
        // 测试内容: rtn_data 推送行情数据更新
        // -------------------------------------------------------------------------
        // 行情数据通过 quotes 字段推送
        //
        // 行情字段说明:
        //   last_price: 最新价
        //   bid_price1/bid_volume1: 买一价/量
        //   ask_price1/ask_volume1: 卖一价/量
        //   volume: 成交量
        //   open_interest: 持仓量
        //   upper_limit/lower_limit: 涨跌停板
        //
        // 价格字段特殊值:
        //   "-": 表示该值尚未确定（如收盘价在收盘前）
        //   NaN: 表示无效数据
        // -------------------------------------------------------------------------
        let quote_update = json!({
            "quotes": {
                "SHFE.cu2512": {
                    "instrument_id": "SHFE.cu2512",
                    "volume_multiple": 5,           // 合约乘数
                    "price_tick": 10.0,             // 最小变动价位
                    "margin": 7550.0,               // 保证金
                    "commission": 2.5,              // 手续费
                    "datetime": "2024-12-17 10:30:00.500000",
                    "last_price": 75500.0,
                    "bid_price1": 75490.0,
                    "bid_volume1": 50,
                    "ask_price1": 75500.0,
                    "ask_volume1": 30,
                    "highest": 75800.0,
                    "lowest": 75200.0,
                    "volume": 123456,
                    "amount": 46296000000.0,        // 成交金额
                    "open_interest": 234567,        // 持仓量
                    "pre_close": 75300.0,
                    "pre_settlement": 75350.0,
                    "open": 75400.0,
                    "upper_limit": 82885.0,         // 涨停价
                    "lower_limit": 67815.0          // 跌停价
                }
            }
        });

        let msg = DiffServerMessage::RtnData {
            data: vec![quote_update],
        };

        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let quote = &parsed["data"][0]["quotes"]["SHFE.cu2512"];
        assert_eq!(quote["last_price"], 75500.0);
        assert_eq!(quote["volume_multiple"], 5);

        // 验证买卖盘
        assert!(quote["bid_price1"].as_f64().unwrap() <= quote["last_price"].as_f64().unwrap());
        assert!(quote["ask_price1"].as_f64().unwrap() >= quote["last_price"].as_f64().unwrap());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // 成交记录测试
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_rtn_data_trade_notification() {
        // -------------------------------------------------------------------------
        // 测试内容: rtn_data 推送成交记录
        // -------------------------------------------------------------------------
        // 订单成交后通过 trades 字段推送成交记录
        //
        // 成交记录 (Trade) 字段:
        //   trade_id: 成交编号（服务端生成）
        //   order_id: 关联的订单ID
        //   exchange_trade_id: 交易所成交编号
        //   price: 成交价格
        //   volume: 成交数量
        //   commission: 手续费
        //   trade_date_time: 成交时间（纳秒时间戳）
        //
        // 成交与订单的关系:
        //   一个订单可能对应多笔成交（部分成交）
        //   trade.order_id 指向原订单
        //   每笔成交都会更新订单的 volume_left
        // -------------------------------------------------------------------------
        let trade_notification = json!({
            "trade": {
                "user123": {
                    "trades": {
                        "order1|12345": {
                            "seqno": 1,
                            "user_id": "user123",
                            "trade_id": "order1|12345",
                            "exchange_id": "SHFE",
                            "instrument_id": "cu2512",
                            "order_id": "order1",
                            "exchange_trade_id": "12345",
                            "direction": "BUY",
                            "offset": "OPEN",
                            "volume": 3.0,                // 本次成交数量
                            "price": 75230.0,             // 成交价格
                            "trade_date_time": 1734444800000000000_i64,  // 纳秒时间戳
                            "commission": 7.5             // 手续费
                        }
                    }
                }
            }
        });

        let msg = DiffServerMessage::RtnData {
            data: vec![trade_notification],
        };

        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let trade = &parsed["data"][0]["trade"]["user123"]["trades"]["order1|12345"];
        assert_eq!(trade["order_id"], "order1");
        assert_eq!(trade["volume"], 3.0);
        assert_eq!(trade["price"], 75230.0);
        assert_eq!(trade["commission"], 7.5);

        // 计算成交金额 (铜合约乘数=5)
        let volume = trade["volume"].as_f64().unwrap();
        let price = trade["price"].as_f64().unwrap();
        let multiplier = 5.0;
        let turnover = volume * price * multiplier;
        assert_eq!(turnover, 1128450.0);  // 3 * 75230 * 5
    }
}
