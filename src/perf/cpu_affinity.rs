//! CPU 亲和性绑定模块
//!
//! @yutiansut @quantaxis
//!
//! 将关键线程绑定到指定 CPU 核心，减少上下文切换和缓存失效
//!
//! 性能目标：
//! - 撮合引擎线程：固定到核心 0（或指定核心）
//! - 行情处理线程：固定到核心 1
//! - 网络 I/O 线程：固定到核心 2-3
//!
//! 使用方式：
//! ```ignore
//! use qaexchange::perf::cpu_affinity::{bind_to_core, CpuAffinityConfig};
//!
//! // 绑定当前线程到核心 0
//! bind_to_core(0).expect("Failed to bind to core 0");
//!
//! // 使用配置绑定
//! let config = CpuAffinityConfig::default();
//! config.bind_matching_engine_thread();
//! ```

use core_affinity::CoreId;
use std::thread;

/// CPU 亲和性配置
#[derive(Debug, Clone)]
pub struct CpuAffinityConfig {
    /// 撮合引擎核心 ID
    pub matching_engine_core: usize,

    /// 行情处理核心 ID
    pub market_data_core: usize,

    /// 网络 I/O 核心 ID 列表
    pub network_io_cores: Vec<usize>,

    /// 存储 I/O 核心 ID
    pub storage_io_core: usize,

    /// 是否启用 CPU 亲和性
    pub enabled: bool,
}

impl Default for CpuAffinityConfig {
    fn default() -> Self {
        Self {
            matching_engine_core: 0,
            market_data_core: 1,
            network_io_cores: vec![2, 3],
            storage_io_core: 4,
            enabled: true,
        }
    }
}

impl CpuAffinityConfig {
    /// 创建新配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 禁用 CPU 亲和性（用于测试或 VM 环境）
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    /// 根据可用核心数自动配置
    pub fn auto_detect() -> Self {
        let available_cores = get_available_cores();
        let num_cores = available_cores.len();

        if num_cores < 2 {
            // 单核或检测失败，禁用亲和性
            return Self::disabled();
        }

        let config = if num_cores >= 8 {
            // 8+ 核心：完整分配
            Self {
                matching_engine_core: 0,
                market_data_core: 1,
                network_io_cores: vec![2, 3],
                storage_io_core: 4,
                enabled: true,
            }
        } else if num_cores >= 4 {
            // 4-7 核心：紧凑分配
            Self {
                matching_engine_core: 0,
                market_data_core: 1,
                network_io_cores: vec![2],
                storage_io_core: 3,
                enabled: true,
            }
        } else {
            // 2-3 核心：最小分配
            Self {
                matching_engine_core: 0,
                market_data_core: 1,
                network_io_cores: vec![1], // 复用
                storage_io_core: 1,        // 复用
                enabled: true,
            }
        };

        log::info!(
            "CPU affinity auto-configured: {} cores available, matching_engine=core{}, market_data=core{}",
            num_cores,
            config.matching_engine_core,
            config.market_data_core
        );

        config
    }

    /// 绑定撮合引擎线程
    pub fn bind_matching_engine_thread(&self) -> Result<(), AffinityError> {
        if !self.enabled {
            return Ok(());
        }
        bind_to_core(self.matching_engine_core)
    }

    /// 绑定行情处理线程
    pub fn bind_market_data_thread(&self) -> Result<(), AffinityError> {
        if !self.enabled {
            return Ok(());
        }
        bind_to_core(self.market_data_core)
    }

    /// 绑定网络 I/O 线程（轮询选择核心）
    pub fn bind_network_io_thread(&self, thread_index: usize) -> Result<(), AffinityError> {
        if !self.enabled || self.network_io_cores.is_empty() {
            return Ok(());
        }
        let core = self.network_io_cores[thread_index % self.network_io_cores.len()];
        bind_to_core(core)
    }

    /// 绑定存储 I/O 线程
    pub fn bind_storage_io_thread(&self) -> Result<(), AffinityError> {
        if !self.enabled {
            return Ok(());
        }
        bind_to_core(self.storage_io_core)
    }
}

/// 亲和性错误
#[derive(Debug, thiserror::Error)]
pub enum AffinityError {
    #[error("Core {0} not available")]
    CoreNotAvailable(usize),

    #[error("Failed to set affinity: {0}")]
    SetAffinityFailed(String),

    #[error("No cores available")]
    NoCoresAvailable,
}

/// 获取可用的 CPU 核心列表
pub fn get_available_cores() -> Vec<CoreId> {
    core_affinity::get_core_ids().unwrap_or_default()
}

/// 获取可用核心数量
pub fn get_core_count() -> usize {
    get_available_cores().len()
}

/// 将当前线程绑定到指定核心
pub fn bind_to_core(core_id: usize) -> Result<(), AffinityError> {
    let cores = get_available_cores();

    if cores.is_empty() {
        return Err(AffinityError::NoCoresAvailable);
    }

    if core_id >= cores.len() {
        return Err(AffinityError::CoreNotAvailable(core_id));
    }

    let target_core = cores[core_id];

    if core_affinity::set_for_current(target_core) {
        log::debug!(
            "Thread {:?} bound to core {}",
            thread::current().id(),
            core_id
        );
        Ok(())
    } else {
        Err(AffinityError::SetAffinityFailed(format!(
            "Failed to bind to core {}",
            core_id
        )))
    }
}

/// 将当前线程绑定到多个核心（允许在这些核心间调度）
pub fn bind_to_cores(core_ids: &[usize]) -> Result<(), AffinityError> {
    if core_ids.is_empty() {
        return Err(AffinityError::NoCoresAvailable);
    }

    // core_affinity 不直接支持多核心绑定，使用第一个核心
    // 更高级的实现可以使用 libc::sched_setaffinity
    bind_to_core(core_ids[0])
}

/// 创建一个绑定到指定核心的线程
pub fn spawn_on_core<F, T>(core_id: usize, name: &str, f: F) -> std::io::Result<thread::JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let thread_name = name.to_string();
    let builder = thread::Builder::new().name(thread_name.clone());

    builder.spawn(move || {
        if let Err(e) = bind_to_core(core_id) {
            log::warn!(
                "Failed to bind thread '{}' to core {}: {}",
                thread_name,
                core_id,
                e
            );
        }
        f()
    })
}

/// 线程亲和性守卫 - RAII 模式
pub struct AffinityGuard {
    _phantom: std::marker::PhantomData<()>,
}

impl AffinityGuard {
    /// 创建守卫并绑定到指定核心
    pub fn new(core_id: usize) -> Result<Self, AffinityError> {
        bind_to_core(core_id)?;
        Ok(Self {
            _phantom: std::marker::PhantomData,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_available_cores() {
        let cores = get_available_cores();
        println!("Available cores: {:?}", cores.len());
        assert!(!cores.is_empty() || cfg!(target_os = "windows")); // Windows CI 可能没有
    }

    #[test]
    fn test_get_core_count() {
        let count = get_core_count();
        println!("Core count: {}", count);
        // 至少应该有 1 个核心
        // 注意：在某些 CI 环境中可能返回 0
    }

    #[test]
    fn test_auto_detect_config() {
        let config = CpuAffinityConfig::auto_detect();
        println!("Auto-detected config: {:?}", config);

        if config.enabled {
            assert!(config.matching_engine_core < get_core_count());
        }
    }

    #[test]
    fn test_bind_to_core() {
        let cores = get_available_cores();
        if cores.is_empty() {
            println!("Skipping test: no cores available");
            return;
        }

        // 绑定到第一个核心
        let result = bind_to_core(0);
        assert!(result.is_ok(), "Failed to bind to core 0: {:?}", result);
    }

    #[test]
    fn test_bind_invalid_core() {
        let cores = get_available_cores();
        let invalid_core = cores.len() + 100;

        let result = bind_to_core(invalid_core);
        assert!(result.is_err());
    }

    #[test]
    fn test_spawn_on_core() {
        let cores = get_available_cores();
        if cores.is_empty() {
            println!("Skipping test: no cores available");
            return;
        }

        let handle = spawn_on_core(0, "test-thread", || {
            println!("Running on core 0");
            42
        });

        match handle {
            Ok(h) => {
                let result = h.join().unwrap();
                assert_eq!(result, 42);
            }
            Err(e) => {
                println!("Could not spawn thread: {}", e);
            }
        }
    }

    #[test]
    fn test_config_disabled() {
        let config = CpuAffinityConfig::disabled();
        assert!(!config.enabled);

        // 禁用时应该直接返回 Ok
        assert!(config.bind_matching_engine_thread().is_ok());
        assert!(config.bind_market_data_thread().is_ok());
    }
}
