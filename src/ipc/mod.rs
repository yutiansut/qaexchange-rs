//! iceoryx2 零拷贝 IPC 模块
//!
//! 使用共享内存实现进程间通信，避免数据拷贝
//!
//! 性能目标：
//! - 延迟：< 1μs (共享内存访问)
//! - 吞吐：> 10M msgs/sec
//! - 零拷贝：完全避免序列化和内存拷贝
//!
//! 架构组件：
//! - manager: IPC 管理器（资源生命周期）
//! - production: 生产级部署支持（健康检查、监控、容量规划）
//! - publisher/subscriber: iceoryx2 发布/订阅实现
//!
//! 注意：当前版本使用条件编译，如果iceoryx2不可用则fallback到crossbeam
//!
//! @yutiansut @quantaxis

pub mod manager;
pub mod production;
pub mod types;

#[cfg(feature = "iceoryx2")]
pub mod publisher;
#[cfg(feature = "iceoryx2")]
pub mod subscriber;

#[cfg(feature = "iceoryx2")]
pub use publisher::IceoryxPublisher;
#[cfg(feature = "iceoryx2")]
pub use subscriber::IceoryxSubscriber;

pub use manager::IceoryxManager;
pub use production::{
    CapacityPlanner, HealthCheckResult, HealthStatus, IpcMetrics, ProductionIpcConfig,
    ProductionIpcManager,
};
pub use types::{IpcMarketData, IpcNotification};

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

/// 生成 iceoryx2 服务名称
///
/// # 参数
/// - `prefix`: 服务名称前缀（如 "qaexchange"）
/// - `topic`: 主题名称（如 "market_data/ticks"）
///
/// # 返回
/// 完整的服务名称（如 "qaexchange/market_data/ticks"）
pub fn make_service_name(prefix: &str, topic: &str) -> String {
    format!("{}/{}", prefix, topic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_service_name() {
        let name = make_service_name("qaexchange", "market_data/ticks");
        assert_eq!(name, "qaexchange/market_data/ticks");
    }
}
