//! iceoryx2 订阅者实现

use super::{make_service_name, IpcConfig, IpcMarketData, IpcNotification};
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

/// iceoryx2 订阅者
pub struct IceoryxSubscriber<T: Copy + std::fmt::Debug> {
    subscriber: Arc<Mutex<Subscriber<ipc::Service, T, ()>>>,
    service_name: String,
}

impl<T: Copy + std::fmt::Debug> IceoryxSubscriber<T> {
    /// 创建新的订阅者
    pub fn new(config: &IpcConfig, topic: &str) -> Result<Self, String> {
        let service_name_str = make_service_name(&config.service_prefix, topic);
        let service_name = ServiceName::new(&service_name_str)
            .map_err(|e| format!("Invalid service name: {:?}", e))?;

        // 打开已存在的服务
        let service = ipc::Service::new(&service_name)
            .publish_subscribe::<T>()
            .open()
            .map_err(|e| format!("Failed to open service: {:?}", e))?;

        let subscriber = service
            .subscriber_builder()
            .create()
            .map_err(|e| format!("Failed to create subscriber: {:?}", e))?;

        Ok(Self {
            subscriber: Arc::new(Mutex::new(subscriber)),
            service_name: service_name_str,
        })
    }

    /// 接收消息（非阻塞）
    pub fn try_receive(&self) -> Result<Option<T>, String> {
        let subscriber = self.subscriber.lock();

        match subscriber.receive() {
            Ok(Some(sample)) => Ok(Some(*sample.payload())),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Receive error: {:?}", e)),
        }
    }

    /// 接收所有待处理消息
    pub fn receive_all(&self) -> Result<Vec<T>, String> {
        let mut messages = Vec::new();

        loop {
            match self.try_receive()? {
                Some(msg) => messages.push(msg),
                None => break,
            }
        }

        Ok(messages)
    }

    /// 获取服务名称
    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}

// 特化：交易通知订阅者
pub type NotificationSubscriber = IceoryxSubscriber<IpcNotification>;

// 特化：市场数据订阅者
pub type MarketDataSubscriber = IceoryxSubscriber<IpcMarketData>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscriber_creation() {
        // 注意：需要先有 publisher 创建服务
        let config = IpcConfig::default();

        // 这会失败，因为服务不存在
        let subscriber = NotificationSubscriber::new(&config, "test/nonexistent");
        assert!(subscriber.is_err());
    }
}
