//! iceoryx2 零拷贝 IPC 模块
//!
//! 使用共享内存实现进程间通信，避免数据拷贝
//!
//! 性能目标：
//! - 延迟：< 1μs (共享内存访问)
//! - 吞吐：> 10M msgs/sec
//! - 零拷贝：完全避免序列化和内存拷贝
//!
//! 注意：当前版本使用条件编译，如果iceoryx2不可用则fallback到crossbeam

pub mod types;

#[cfg(feature = "iceoryx2")]
pub mod publisher;
#[cfg(feature = "iceoryx2")]
pub mod subscriber;

#[cfg(feature = "iceoryx2")]
pub use publisher::IceoryxPublisher;
#[cfg(feature = "iceoryx2")]
pub use subscriber::IceoryxSubscriber;

pub use types::{IpcNotification, IpcMarketData};

/// iceoryx2 服务配置
#[derive(Debug, Clone)]
pub struct IpcConfig {
    /// 服务名称前缀
    pub service_prefix: String,

    /// 最大订阅者数量
    pub max_subscribers: usize,

    /// 消息队列大小
    pub queue_capacity: usize,

    /// 消息最大大小（字节）
    pub max_message_size: usize,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            service_prefix: "qaexchange".to_string(),
            max_subscribers: 1000,
            queue_capacity: 1024,
            max_message_size: 4096,
        }
    }
}
