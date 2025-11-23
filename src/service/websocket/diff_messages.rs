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
    /// 业务信息截面更新（rtn_data）
    RtnData {
        data: Vec<Value>, // JSON Merge Patch 数组
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_peek_message_serialization() {
        let msg = DiffClientMessage::PeekMessage;
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["aid"], "peek_message");
    }

    #[test]
    fn test_rtn_data_serialization() {
        let msg = DiffServerMessage::RtnData {
            data: vec![json!({"balance": 100000.0})],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"aid\":\"rtn_data\""));
    }

    #[test]
    fn test_insert_order_serialization() {
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
    }
}
