// OLTP → OLAP 异步转换系统
//
// 架构设计：
//
// ┌─────────────────────────────────────────────────────────────────┐
// │                         OLTP 主进程                              │
// │                    (不受转换影响，保证实时性)                      │
// │                  WAL → MemTable → SSTable                        │
// └─────────────────────────┬───────────────────────────────────────┘
//                           │
//                           ▼
//                 ┌─────────────────────┐
//                 │   转换调度器         │
//                 │   (Scheduler)       │
//                 │  - 定期扫描          │
//                 │  - 批量分组          │
//                 │  - 提交任务          │
//                 │  - 僵尸任务恢复      │
//                 └──────────┬──────────┘
//                            │
//                            ▼
//                 ┌─────────────────────┐
//                 │   任务队列           │
//                 │  (Channel)          │
//                 └──────────┬──────────┘
//                            │
//            ┌───────────────┼───────────────┐
//            │               │               │
//        ┌───▼────┐    ┌────▼───┐    ┌─────▼───┐
//        │Worker 1│    │Worker 2│    │Worker N │
//        │ 品种 A  │    │ 品种 B  │    │ 品种 X  │
//        └───┬────┘    └────┬───┘    └─────┬───┘
//            │              │               │
//            └──────────────┼───────────────┘
//                           │
//                           ▼
//              ┌────────────────────────┐
//              │   OLAP Parquet 文件     │
//              │  (压缩、列式、持久化)    │
//              └────────────────────────┘
//
// 性能保证：
// 1. 独立线程池，不占用 OLTP 资源
// 2. 批量转换，减少 I/O
// 3. 流式处理，避免内存暴涨
// 4. I/O 限流，避免影响 OLTP
//
// 错误恢复：
// 1. 转换前校验：检查源文件完整性
// 2. 原子性写入：临时文件 + rename
// 3. 状态持久化：转换记录落盘
// 4. 失败重试：指数退避（1s→2s→4s→8s）
// 5. 源文件保护：转换完成前不删除

pub mod metadata;
pub mod scheduler;
pub mod worker;

pub use metadata::{ConversionMetadata, ConversionRecord, ConversionStatus, ConversionStats};
pub use scheduler::{ConversionScheduler, SchedulerConfig, ConversionTask};
pub use worker::{ConversionWorker, WorkerConfig, WorkerPool};

use std::sync::{Arc, Mutex};
use std::path::PathBuf;

/// 转换系统管理器
///
/// 整合调度器和 Worker 线程池，提供统一的启动/停止接口
pub struct ConversionManager {
    metadata: Arc<Mutex<ConversionMetadata>>,
    scheduler: Arc<ConversionScheduler>,
    worker_pool: Option<WorkerPool>,
    scheduler_handle: Option<std::thread::JoinHandle<()>>,
}

impl ConversionManager {
    /// 创建转换系统管理器
    pub fn new(
        storage_base_path: PathBuf,
        metadata_path: PathBuf,
        scheduler_config: SchedulerConfig,
        worker_config: WorkerConfig,
    ) -> Result<Self, String> {
        // 加载或创建元数据
        let metadata = Arc::new(Mutex::new(ConversionMetadata::load(metadata_path)?));

        // 创建调度器
        let scheduler = Arc::new(ConversionScheduler::new(
            scheduler_config,
            metadata.clone(),
            storage_base_path,
        ));

        // 创建 Worker 线程池
        let worker_pool = WorkerPool::new(
            worker_config,
            metadata.clone(),
            scheduler.task_receiver(),
        );

        Ok(Self {
            metadata,
            scheduler,
            worker_pool: Some(worker_pool),
            scheduler_handle: None,
        })
    }

    /// 启动转换系统
    pub fn start(&mut self) {
        log::info!("Starting conversion system...");

        // 启动 Worker 线程池
        if let Some(mut pool) = self.worker_pool.take() {
            pool.start();
            self.worker_pool = Some(pool);
        }

        // 启动调度器线程
        let scheduler = self.scheduler.clone();
        let handle = std::thread::Builder::new()
            .name("conversion-scheduler".to_string())
            .spawn(move || {
                scheduler.run();
            })
            .expect("Failed to spawn scheduler thread");

        self.scheduler_handle = Some(handle);

        log::info!("Conversion system started");
    }

    /// 停止转换系统
    pub fn stop(mut self) {
        log::info!("Stopping conversion system...");

        // 停止 Worker 线程池
        if let Some(pool) = self.worker_pool.take() {
            pool.stop();
        }

        // 调度器线程会在下次扫描后自然退出
        // 这里不主动中断，避免任务丢失

        log::info!("Conversion system stopped");
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> ConversionStats {
        let metadata = self.metadata.lock().unwrap();
        metadata.get_stats()
    }

    /// 手动触发转换（用于测试或立即转换）
    pub fn trigger_conversion(
        &self,
        instrument_id: &str,
        oltp_sstables: Vec<PathBuf>,
    ) -> Result<(), String> {
        let mut metadata = self.metadata.lock().unwrap();

        // 生成 OLAP Parquet 文件名
        let storage_base = self.scheduler.storage_base_path.clone();
        let olap_dir = storage_base.join(instrument_id).join("olap");

        std::fs::create_dir_all(&olap_dir)
            .map_err(|e| format!("Create OLAP dir failed: {}", e))?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let olap_parquet = olap_dir.join(format!("manual_{}.parquet", timestamp));

        // 创建转换记录
        let id = metadata.allocate_id();
        let record = ConversionRecord::new(
            id,
            instrument_id.to_string(),
            oltp_sstables,
            olap_parquet,
        );

        log::info!("Manually triggering conversion task {}", id);

        // 保存记录
        metadata.add_record(record.clone())?;

        // 提交任务
        self.scheduler
            .task_sender()
            .send(ConversionTask { record })
            .map_err(|e| format!("Send task failed: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_conversion_manager_creation() {
        let tmp_dir = tempdir().unwrap();
        let storage_path = tmp_dir.path().to_path_buf();
        let metadata_path = tmp_dir.path().join("metadata.json");

        let scheduler_config = SchedulerConfig::default();
        let worker_config = WorkerConfig::default();

        let manager = ConversionManager::new(
            storage_path,
            metadata_path,
            scheduler_config,
            worker_config,
        );

        assert!(manager.is_ok());
    }

    #[test]
    fn test_conversion_manager_stats() {
        let tmp_dir = tempdir().unwrap();
        let storage_path = tmp_dir.path().to_path_buf();
        let metadata_path = tmp_dir.path().join("metadata.json");

        let scheduler_config = SchedulerConfig::default();
        let worker_config = WorkerConfig::default();

        let manager = ConversionManager::new(
            storage_path,
            metadata_path,
            scheduler_config,
            worker_config,
        )
        .unwrap();

        let stats = manager.get_stats();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.success, 0);
    }
}
