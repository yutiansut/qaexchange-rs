//! QAEXCHANGE 服务器主程序

use qaexchange::utils::logger;
use qaexchange::exchange::{AccountManager, InstrumentRegistry};
use qaexchange::matching::engine::ExchangeMatchingEngine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    logger::init_logger();

    log::info!("Starting QAEXCHANGE server...");

    // 创建核心组件
    let account_mgr = AccountManager::new();
    let instrument_registry = InstrumentRegistry::new();
    let matching_engine = ExchangeMatchingEngine::new();

    log::info!("Account manager initialized");
    log::info!("Instrument registry initialized");
    log::info!("Matching engine initialized");

    // TODO: 启动 WebSocket 和 HTTP 服务

    log::info!("QAEXCHANGE server started successfully");

    // 保持运行
    tokio::signal::ctrl_c().await?;

    log::info!("Shutting down QAEXCHANGE server...");

    Ok(())
}
