//! 完整交易演示 - HTTP + WebSocket 客户端
//!
//! 演示如何使用 QAExchange 的 HTTP API 和 WebSocket API 进行交易
//!
//! 运行步骤：
//! 1. 启动服务器: cargo run --bin qaexchange-server
//! 2. 运行客户端: cargo run --example full_trading_demo

use reqwest;
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};
use std::time::Duration;

const HTTP_BASE_URL: &str = "http://127.0.0.1:8094";
const WS_URL: &str = "ws://127.0.0.1:8095/ws?user_id=demo_user";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║          QAExchange 完整交易演示                                    ║");
    println!("║  HTTP API (开户/查询) + WebSocket (实时交易)                       ║");
    println!("╚════════════════════════════════════════════════════════════════════╝\n");

    // ============================================================
    // Step 1: 健康检查
    // ============================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔍 Step 1: 健康检查");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let client = reqwest::Client::new();

    match client.get(&format!("{}/health", HTTP_BASE_URL)).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                println!("✅ Server is running at {}", HTTP_BASE_URL);
            } else {
                eprintln!("❌ Server health check failed: {}", resp.status());
                return Ok(());
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to connect to server: {}", e);
            eprintln!("   Please start the server first: cargo run --bin qaexchange-server");
            return Ok(());
        }
    }

    // ============================================================
    // Step 2: HTTP 开户
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("👤 Step 2: HTTP API - 开户");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let open_account_req = json!({
        "user_id": "demo_user",
        "user_name": "Demo User",
        "init_cash": 1000000.0,
        "account_type": "individual",
        "password": "demo123"
    });

    println!("Request:");
    println!("{}", serde_json::to_string_pretty(&open_account_req)?);

    let resp = client
        .post(&format!("{}/api/account/open", HTTP_BASE_URL))
        .json(&open_account_req)
        .send()
        .await?;

    if resp.status().is_success() {
        let result: serde_json::Value = resp.json().await?;
        println!("\n✅ Account opened successfully:");
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("⚠️  Account may already exist (this is OK)");
    }

    // ============================================================
    // Step 3: HTTP 查询账户
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("💰 Step 3: HTTP API - 查询账户");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let resp = client
        .get(&format!("{}/api/account/demo_user", HTTP_BASE_URL))
        .send()
        .await?;

    if resp.status().is_success() {
        let result: serde_json::Value = resp.json().await?;
        println!("Account Info:");
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("❌ Failed to query account");
    }

    // ============================================================
    // Step 4: HTTP 提交订单
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📝 Step 4: HTTP API - 提交订单");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let submit_order_req = json!({
        "user_id": "demo_user",
        "instrument_id": "IF2501",
        "direction": "BUY",
        "offset": "OPEN",
        "volume": 2.0,
        "price": 3800.0,
        "order_type": "LIMIT"
    });

    println!("Order Request:");
    println!("{}", serde_json::to_string_pretty(&submit_order_req)?);

    let resp = client
        .post(&format!("{}/api/order/submit", HTTP_BASE_URL))
        .json(&submit_order_req)
        .send()
        .await?;

    let mut order_id = String::new();
    if resp.status().is_success() {
        let result: serde_json::Value = resp.json().await?;
        println!("\n✅ Order submitted:");
        println!("{}", serde_json::to_string_pretty(&result)?);

        if let Some(data) = result.get("data") {
            if let Some(oid) = data.get("order_id") {
                order_id = oid.as_str().unwrap_or("").to_string();
            }
        }
    } else {
        println!("❌ Failed to submit order: {}", resp.status());
    }

    // ============================================================
    // Step 5: WebSocket 连接
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔌 Step 5: WebSocket - 建立连接");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("Connecting to {}...", WS_URL);

    let (ws_stream, _) = match connect_async(WS_URL).await {
        Ok(conn) => {
            println!("✅ WebSocket connected");
            conn
        }
        Err(e) => {
            eprintln!("❌ Failed to connect WebSocket: {}", e);
            return Ok(());
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // ============================================================
    // Step 6: WebSocket 认证
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔐 Step 6: WebSocket - 认证");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let auth_msg = json!({
        "type": "auth",
        "user_id": "demo_user",
        "token": "demo_token"
    });

    println!("Sending auth message...");
    write.send(Message::Text(auth_msg.to_string())).await?;

    // 启动消息接收任务
    let read_handle = tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let msg_type = json.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");

                        match msg_type {
                            "auth_response" => {
                                println!("✅ Auth response: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
                            }
                            "order_response" => {
                                println!("📝 Order response: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
                            }
                            "trade" => {
                                println!("🤝 Trade notification: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
                            }
                            "account_update" => {
                                println!("💰 Account update: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
                            }
                            "order_status" => {
                                println!("📊 Order status: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
                            }
                            "pong" => {
                                println!("🏓 Pong received");
                            }
                            _ => {
                                println!("📨 Message: {}", text);
                            }
                        }
                    } else {
                        println!("📨 Raw message: {}", text);
                    }
                }
                Ok(Message::Close(_)) => {
                    println!("🔴 WebSocket closed");
                    break;
                }
                Err(e) => {
                    eprintln!("❌ WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    // ============================================================
    // Step 7: WebSocket 提交订单
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📝 Step 7: WebSocket - 提交订单");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let ws_order_msg = json!({
        "type": "submit_order",
        "instrument_id": "IF2501",
        "direction": "SELL",
        "offset": "CLOSE",
        "volume": 1.0,
        "price": 3805.0,
        "order_type": "LIMIT"
    });

    println!("Submitting order via WebSocket...");
    write.send(Message::Text(ws_order_msg.to_string())).await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // ============================================================
    // Step 8: WebSocket 查询账户
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("💰 Step 8: WebSocket - 查询账户");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let query_account_msg = json!({
        "type": "query_account"
    });

    println!("Querying account via WebSocket...");
    write.send(Message::Text(query_account_msg.to_string())).await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // ============================================================
    // Step 9: WebSocket 心跳
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🏓 Step 9: WebSocket - 心跳");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let ping_msg = json!({
        "type": "ping"
    });

    println!("Sending ping...");
    write.send(Message::Text(ping_msg.to_string())).await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // ============================================================
    // Step 10: 最终查询账户（HTTP）
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 Step 10: HTTP API - 最终账户状态");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let resp = client
        .get(&format!("{}/api/account/demo_user", HTTP_BASE_URL))
        .send()
        .await?;

    if resp.status().is_success() {
        let result: serde_json::Value = resp.json().await?;
        println!("Final Account State:");
        println!("{}", serde_json::to_string_pretty(&result)?);
    }

    // ============================================================
    // 关闭
    // ============================================================
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Demo completed!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("📋 Summary:");
    println!("   • Opened account via HTTP");
    println!("   • Submitted order via HTTP");
    println!("   • Connected WebSocket and authenticated");
    println!("   • Submitted order via WebSocket");
    println!("   • Received real-time notifications");
    println!("   • Queried account via WebSocket");
    println!("   • Verified final state via HTTP");

    println!("\n💡 Next Steps:");
    println!("   • Try the stress test: cargo run --example stress_test");
    println!("   • Monitor storage: ls -lh /tmp/qaexchange/storage/");
    println!("   • View docs: docs/DECOUPLED_STORAGE_ARCHITECTURE.md");

    // 等待一下让所有消息处理完
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 关闭 WebSocket
    write.send(Message::Close(None)).await?;
    let _ = read_handle.await;

    Ok(())
}
