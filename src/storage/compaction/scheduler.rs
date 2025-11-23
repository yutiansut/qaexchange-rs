//! Compaction 调度器 - 后台线程管理

use super::{CompactionConfig, LeveledCompaction, SSTableInfo};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{interval, Duration};

/// Compaction 调度器
pub struct CompactionScheduler {
    /// Leveled compaction 实现
    compaction: Arc<LeveledCompaction>,

    /// 所有层级的 SSTable（level → sstables）
    level_sstables: Arc<RwLock<HashMap<usize, Vec<SSTableInfo>>>>,

    /// 配置
    config: CompactionConfig,

    /// 是否正在运行
    running: Arc<parking_lot::RwLock<bool>>,
}

impl CompactionScheduler {
    pub fn new(base_path: PathBuf, config: CompactionConfig) -> Self {
        let compaction = Arc::new(LeveledCompaction::new(base_path, config.clone()));

        Self {
            compaction,
            level_sstables: Arc::new(RwLock::new(HashMap::new())),
            config,
            running: Arc::new(parking_lot::RwLock::new(false)),
        }
    }

    /// 注册一个新的 SSTable（当 MemTable flush 时调用）
    pub fn register_sstable(&self, info: SSTableInfo) {
        let mut levels = self.level_sstables.write();
        levels.entry(info.level).or_insert_with(Vec::new).push(info);
    }

    /// 获取所有层级的 SSTable
    pub fn get_level_sstables(&self) -> HashMap<usize, Vec<SSTableInfo>> {
        self.level_sstables.read().clone()
    }

    /// 启动后台 compaction 线程
    pub fn start(&self) {
        {
            let mut running = self.running.write();
            if *running {
                log::warn!("Compaction scheduler already running");
                return;
            }
            *running = true;
        }

        let compaction = self.compaction.clone();
        let level_sstables = self.level_sstables.clone();
        let running = self.running.clone();
        let check_interval = Duration::from_secs(10); // 每 10 秒检查一次

        tokio::spawn(async move {
            let mut ticker = interval(check_interval);

            log::info!(
                "Compaction scheduler started (interval: {:?})",
                check_interval
            );

            loop {
                ticker.tick().await;

                if !*running.read() {
                    log::info!("Compaction scheduler stopped");
                    break;
                }

                // 检查是否需要 compaction
                let levels = level_sstables.read().clone();
                if let Some(task) = compaction.should_compact(&levels) {
                    log::info!(
                        "Triggering compaction: L{} → L{} ({} files)",
                        task.source_level,
                        task.target_level,
                        task.sstables.len()
                    );

                    // 执行 compaction
                    match compaction.execute_compaction(task.clone()) {
                        Ok(result) => {
                            // 更新 SSTable 列表
                            let mut levels = level_sstables.write();

                            // 移除旧的 SSTable
                            for obsolete_path in &result.obsolete_sstables {
                                for (_, sstables) in levels.iter_mut() {
                                    sstables.retain(|info| info.file_path != *obsolete_path);
                                }

                                // 删除文件
                                if let Err(e) = std::fs::remove_file(obsolete_path) {
                                    log::warn!(
                                        "Failed to delete obsolete SSTable {}: {}",
                                        obsolete_path,
                                        e
                                    );
                                }
                            }

                            // 添加新的 SSTable
                            levels
                                .entry(result.new_sstable.level)
                                .or_insert_with(Vec::new)
                                .push(result.new_sstable);

                            log::info!(
                                "Compaction succeeded: merged {} entries, deleted {} files",
                                result.merged_entries,
                                result.obsolete_sstables.len()
                            );
                        }
                        Err(e) => {
                            log::error!("Compaction failed: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// 停止后台 compaction 线程
    pub fn stop(&self) {
        let mut running = self.running.write();
        *running = false;
        log::info!("Stopping compaction scheduler...");
    }

    /// 手动触发 compaction
    pub fn trigger_compaction(&self) -> Result<(), String> {
        let levels = self.level_sstables.read().clone();

        if let Some(task) = self.compaction.should_compact(&levels) {
            let result = self.compaction.execute_compaction(task)?;

            // 更新 SSTable 列表
            let mut levels = self.level_sstables.write();

            // 移除旧的 SSTable
            for obsolete_path in &result.obsolete_sstables {
                for (_, sstables) in levels.iter_mut() {
                    sstables.retain(|info| info.file_path != *obsolete_path);
                }

                // 删除文件
                if let Err(e) = std::fs::remove_file(obsolete_path) {
                    log::warn!("Failed to delete obsolete SSTable {}: {}", obsolete_path, e);
                }
            }

            // 添加新的 SSTable
            levels
                .entry(result.new_sstable.level)
                .or_insert_with(Vec::new)
                .push(result.new_sstable);

            Ok(())
        } else {
            Err("No compaction needed".to_string())
        }
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> CompactionStats {
        let levels = self.level_sstables.read();

        let mut level_counts = HashMap::new();
        let mut level_sizes = HashMap::new();

        for (&level, sstables) in levels.iter() {
            level_counts.insert(level, sstables.len());
            let total_size: u64 = sstables.iter().map(|s| s.file_size).sum();
            level_sizes.insert(level, total_size);
        }

        CompactionStats {
            level_counts,
            level_sizes,
        }
    }
}

/// Compaction 统计信息
#[derive(Debug, Clone)]
pub struct CompactionStats {
    /// 每层的文件数
    pub level_counts: HashMap<usize, usize>,

    /// 每层的总大小（bytes）
    pub level_sizes: HashMap<usize, u64>,
}
