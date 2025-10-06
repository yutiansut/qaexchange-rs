//! iceoryx2 管理器
//!
//! 负责管理 iceoryx2 发布者的生命周期，提供统一的接口

#[cfg(feature = "iceoryx2")]
use super::publisher::{MarketDataPublisher, NotificationPublisher};
use super::{IpcConfig, IpcMarketData, IpcNotification};
use std::sync::Arc;
use parking_lot::RwLock;

/// iceoryx2 管理器
pub struct IceoryxManager {
    /// 配置
    config: IpcConfig,

    /// 市场数据发布者（可选）
    #[cfg(feature = "iceoryx2")]
    market_data_publisher: Option<Arc<MarketDataPublisher>>,

    /// 交易通知发布者（可选）
    #[cfg(feature = "iceoryx2")]
    notification_publisher: Option<Arc<NotificationPublisher>>,

    /// 统计：发送的市场数据消息数
    market_data_count: Arc<RwLock<u64>>,

    /// 统计：发送的交易通知消息数
    notification_count: Arc<RwLock<u64>>,
}

impl IceoryxManager {
    /// 创建新的管理器
    pub fn new(config: IpcConfig) -> Self {
        log::info!("Creating IceoryxManager with config: {:?}", config);

        Self {
            config,
            #[cfg(feature = "iceoryx2")]
            market_data_publisher: None,
            #[cfg(feature = "iceoryx2")]
            notification_publisher: None,
            market_data_count: Arc::new(RwLock::new(0)),
            notification_count: Arc::new(RwLock::new(0)),
        }
    }

    /// 启动市场数据发布者
    #[cfg(feature = "iceoryx2")]
    pub fn start_market_data_publisher(&mut self) -> Result<(), String> {
        if self.market_data_publisher.is_some() {
            return Err("Market data publisher already started".to_string());
        }

        log::info!("Starting market data publisher...");

        let publisher = MarketDataPublisher::new(&self.config, "market_data/ticks")?;
        self.market_data_publisher = Some(Arc::new(publisher));

        if let Some(ref publisher) = self.market_data_publisher {
            log::info!("✅ Market data publisher started (service: {})", publisher.service_name());
        }

        Ok(())
    }

    /// 启动交易通知发布者
    #[cfg(feature = "iceoryx2")]
    pub fn start_notification_publisher(&mut self) -> Result<(), String> {
        if self.notification_publisher.is_some() {
            return Err("Notification publisher already started".to_string());
        }

        log::info!("Starting notification publisher...");

        let publisher = NotificationPublisher::new(&self.config, "notifications/trades")?;
        self.notification_publisher = Some(Arc::new(publisher));

        if let Some(ref publisher) = self.notification_publisher {
            log::info!("✅ Notification publisher started (service: {})", publisher.service_name());
        }

        Ok(())
    }

    /// 发布市场数据（零拷贝）
    #[cfg(feature = "iceoryx2")]
    pub fn publish_market_data(&self, data: &IpcMarketData) -> Result<(), String> {
        if let Some(ref publisher) = self.market_data_publisher {
            publisher.publish(data)?;
            *self.market_data_count.write() += 1;
            Ok(())
        } else {
            Err("Market data publisher not started".to_string())
        }
    }

    /// 发布市场数据（无 iceoryx2 特性时为空操作）
    #[cfg(not(feature = "iceoryx2"))]
    pub fn publish_market_data(&self, _data: &IpcMarketData) -> Result<(), String> {
        // 无操作：iceoryx2 未启用
        Ok(())
    }

    /// 发布交易通知（零拷贝）
    #[cfg(feature = "iceoryx2")]
    pub fn publish_notification(&self, notification: &IpcNotification) -> Result<(), String> {
        if let Some(ref publisher) = self.notification_publisher {
            publisher.publish(notification)?;
            *self.notification_count.write() += 1;
            Ok(())
        } else {
            Err("Notification publisher not started".to_string())
        }
    }

    /// 发布交易通知（无 iceoryx2 特性时为空操作）
    #[cfg(not(feature = "iceoryx2"))]
    pub fn publish_notification(&self, _notification: &IpcNotification) -> Result<(), String> {
        // 无操作：iceoryx2 未启用
        Ok(())
    }

    /// 获取市场数据发布统计
    pub fn get_market_data_count(&self) -> u64 {
        *self.market_data_count.read()
    }

    /// 获取交易通知发布统计
    pub fn get_notification_count(&self) -> u64 {
        *self.notification_count.read()
    }

    /// 获取订阅者数量统计
    #[cfg(feature = "iceoryx2")]
    pub fn get_subscriber_counts(&self) -> (usize, usize) {
        let market_data_subs = if let Some(ref p) = self.market_data_publisher {
            p.subscriber_count()
        } else {
            0
        };

        let notification_subs = if let Some(ref p) = self.notification_publisher {
            p.subscriber_count()
        } else {
            0
        };

        (market_data_subs, notification_subs)
    }

    /// 获取订阅者数量统计（无 iceoryx2 特性）
    #[cfg(not(feature = "iceoryx2"))]
    pub fn get_subscriber_counts(&self) -> (usize, usize) {
        (0, 0)
    }

    /// 打印统计信息
    pub fn print_stats(&self) {
        let market_data_count = self.get_market_data_count();
        let notification_count = self.get_notification_count();
        let (md_subs, notif_subs) = self.get_subscriber_counts();

        log::info!("📊 iceoryx2 Statistics:");
        log::info!("  Market Data: {} messages published, {} subscribers",
            market_data_count, md_subs);
        log::info!("  Notifications: {} messages published, {} subscribers",
            notification_count, notif_subs);
    }
}

impl Drop for IceoryxManager {
    fn drop(&mut self) {
        log::info!("Shutting down IceoryxManager");
        self.print_stats();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_creation() {
        let config = IpcConfig::default();
        let manager = IceoryxManager::new(config);

        assert_eq!(manager.get_market_data_count(), 0);
        assert_eq!(manager.get_notification_count(), 0);
    }

    #[test]
    #[cfg(feature = "iceoryx2")]
    fn test_start_publishers() {
        let config = IpcConfig::default();
        let mut manager = IceoryxManager::new(config);

        // 启动市场数据发布者
        let result = manager.start_market_data_publisher();
        assert!(result.is_ok());

        // 再次启动应该失败
        let result = manager.start_market_data_publisher();
        assert!(result.is_err());

        // 启动交易通知发布者
        let result = manager.start_notification_publisher();
        assert!(result.is_ok());
    }
}
