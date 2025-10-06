//! iceoryx2 发布者实现

use super::{IpcConfig, IpcNotification, IpcMarketData, make_service_name};
use iceoryx2::prelude::*;
use iceoryx2::port::publisher::Publisher;
use std::sync::Arc;
use parking_lot::Mutex;

/// iceoryx2 发布者
pub struct IceoryxPublisher<T: Copy + std::fmt::Debug> {
    publisher: Arc<Mutex<Publisher<ipc::Service, T, ()>>>,
    service_name: String,
}

impl<T: Copy + std::fmt::Debug> IceoryxPublisher<T> {
    /// 创建新的发布者
    pub fn new(config: &IpcConfig, topic: &str) -> Result<Self, String> {
        let service_name_str = make_service_name(&config.service_prefix, topic);
        let service_name = ServiceName::new(&service_name_str)
            .map_err(|e| format!("Invalid service name: {:?}", e))?;

        // 创建或打开 iceoryx2 服务
        let service = ipc::Service::new(&service_name)
            .publish_subscribe::<T>()
            .max_subscribers(config.max_subscribers)
            .subscriber_max_buffer_size(config.queue_capacity)
            .create()
            .map_err(|e| format!("Failed to create service: {:?}", e))?;

        let publisher = service
            .publisher_builder()
            .create()
            .map_err(|e| format!("Failed to create publisher: {:?}", e))?;

        Ok(Self {
            publisher: Arc::new(Mutex::new(publisher)),
            service_name: service_name_str,
        })
    }

    /// 发布消息（零拷贝）
    pub fn publish(&self, data: &T) -> Result<(), String> {
        let mut publisher = self.publisher.lock();

        // 获取共享内存样本
        let sample = publisher
            .loan_uninit()
            .map_err(|e| format!("Failed to loan sample: {:?}", e))?;

        // 写入数据到共享内存
        let sample = sample.write_payload(*data);

        // 发送（零拷贝，只传递指针）
        sample
            .send()
            .map_err(|e| format!("Failed to send: {:?}", e))?;

        Ok(())
    }

    /// 获取当前订阅者数量
    pub fn subscriber_count(&self) -> usize {
        let publisher = self.publisher.lock();
        publisher.subscriber_connections().count()
    }

    /// 获取服务名称
    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}

// 特化：交易通知发布者
pub type NotificationPublisher = IceoryxPublisher<IpcNotification>;

// 特化：市场数据发布者
pub type MarketDataPublisher = IceoryxPublisher<IpcMarketData>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publisher_creation() {
        let config = IpcConfig::default();
        let publisher = NotificationPublisher::new(&config, "test/notifications");

        assert!(publisher.is_ok());
    }
}
