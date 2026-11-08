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

/// 从 level 索引里摘掉**磁盘上已不存在**的 SSTable,返回被摘掉的路径。
///
/// 抽成自由函数是为了可单测:调度逻辑本体活在 `start()` 的
/// `thread::spawn` 闭包里,那里没法直接断言。@yutiansut @quantaxis
///
/// 只看 `candidates`(本次 compaction 任务涉及的文件),不全表扫描 ——
/// 每轮对整个 level 索引做 stat 会带来无谓的 I/O。
fn drop_missing_sstables(
    levels: &mut HashMap<usize, Vec<SSTableInfo>>,
    candidates: &[SSTableInfo],
) -> Vec<String> {
    let missing: Vec<String> = candidates
        .iter()
        .filter(|info| !std::path::Path::new(&info.file_path).exists())
        .map(|info| info.file_path.clone())
        .collect();

    for path in &missing {
        for (_, sstables) in levels.iter_mut() {
            sstables.retain(|info| info.file_path != *path);
        }
    }
    missing
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
        levels.entry(info.level).or_default().push(info);
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
                                .or_default()
                                .push(result.new_sstable);

                            log::info!(
                                "Compaction succeeded: merged {} entries, deleted {} files",
                                result.merged_entries,
                                result.obsolete_sstables.len()
                            );
                        }
                        Err(e) => {
                            // ✨ 失败后必须把「文件已不在」的条目摘掉,否则无限重试
                            // @yutiansut @quantaxis
                            //
                            // 原来这里只打一行 error 就完事,`level_sstables` 里那条
                            // 指向已消失文件的记录**永远留着**,下一轮 `should_compact`
                            // 又选中同一个任务 —— 实测生产日志:
                            //   `0000000001.sst` 一个文件失败 **2089 次**(6 小时,约每 10 秒一次),
                            //   占全部 ERROR 输出的 **93%**(2081/2233),把真正的错误彻底淹没。
                            //
                            // 文件为什么会消失:转换 worker 把数据转成 Parquet 后,
                            // 把 `NNNN.sst` **改名**成 `NNNN.archived`
                            // (`storage/conversion/worker.rs:385`),而它**不通知本调度器**。
                            // 目录里只剩 `0000000005.archived` 就是这么来的。
                            //
                            // 这里做的是**自愈**而非「修好那条通知链」:文件不存在的条目
                            // 永远不可能 compact 成功,留着一定是错的 —— 无论它是被转换归档、
                            // 被人手删、还是被外部清理脚本删掉的。
                            let missing = {
                                let mut levels = level_sstables.write();
                                drop_missing_sstables(&mut levels, &task.sstables)
                            };

                            if missing.is_empty() {
                                // 文件都在 —— 是真的 compaction 故障,该报 error
                                log::error!("Compaction failed: {}", e);
                            } else {
                                // 源文件已消失是**预期内**的(转换归档),不是服务端故障 ⇒ warn
                                log::warn!(
                                    "Compaction skipped: {} source SSTable(s) no longer on disk, \
                                     dropped from level index: {:?} (原因: {})",
                                    missing.len(),
                                    missing,
                                    e
                                );
                            }
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
                .or_default()
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

#[cfg(test)]
mod missing_sstable_tests {
    //! `drop_missing_sstables` 的闸门。@yutiansut @quantaxis
    //!
    //! 这段代码防的是一个**已在生产发生**的故障:转换 worker 把 `NNNN.sst`
    //! 改名成 `NNNN.archived`(`storage/conversion/worker.rs:385`)却不通知本调度器,
    //! 于是 level 索引里留着指向已消失文件的条目,每轮重新调度、每轮 ENOENT。
    //! 实测:单个文件失败 **2089 次 / 6 小时**,占全部 ERROR 输出的 93%。
    //!
    //! 最容易写错的方向是**摘过头** —— 把文件还在的条目也删了会真丢数据,
    //! 所以下面每个用例都带「不该动的条目必须原样还在」这条断言。

    use super::*;

    fn info(path: &str, level: usize) -> SSTableInfo {
        SSTableInfo {
            file_path: path.to_string(),
            file_size: 1,
            min_key: vec![],
            max_key: vec![],
            level,
            entry_count: 1,
            min_timestamp: 0,
            max_timestamp: 0,
        }
    }

    fn paths(levels: &HashMap<usize, Vec<SSTableInfo>>, level: usize) -> Vec<String> {
        let mut v: Vec<String> = levels
            .get(&level)
            .map(|s| s.iter().map(|i| i.file_path.clone()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    #[test]
    fn drops_only_the_files_that_vanished() {
        let dir = tempfile::tempdir().unwrap();
        let alive = dir.path().join("0000000002.sst");
        std::fs::write(&alive, b"x").unwrap();
        let alive = alive.to_string_lossy().to_string();
        // 模拟被转换 worker 改名成 .archived 的那个:路径上已无文件
        let gone = dir.path().join("0000000001.sst").to_string_lossy().to_string();

        let mut levels: HashMap<usize, Vec<SSTableInfo>> = HashMap::new();
        levels.insert(0, vec![info(&gone, 0), info(&alive, 0)]);

        let task = vec![info(&gone, 0), info(&alive, 0)];
        let dropped = drop_missing_sstables(&mut levels, &task);

        assert_eq!(dropped, vec![gone.clone()], "只应摘掉消失的那个");
        assert_eq!(paths(&levels, 0), vec![alive.clone()], "存在的文件必须留下");
    }

    #[test]
    fn removes_from_every_level_not_just_the_task_level() {
        // 同一路径可能被登记在多个 level(历史遗留 / 重复注册),必须全部清掉,
        // 否则下一轮换个 level 又能选中它 —— 无限重试就没真正治好
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("g.sst").to_string_lossy().to_string();
        let alive = dir.path().join("a.sst");
        std::fs::write(&alive, b"x").unwrap();
        let alive = alive.to_string_lossy().to_string();

        let mut levels: HashMap<usize, Vec<SSTableInfo>> = HashMap::new();
        levels.insert(0, vec![info(&gone, 0), info(&alive, 0)]);
        levels.insert(1, vec![info(&gone, 1)]);

        let dropped = drop_missing_sstables(&mut levels, &[info(&gone, 0)]);
        assert_eq!(dropped.len(), 1);
        assert!(paths(&levels, 0).iter().all(|p| p != &gone), "L0 未清除");
        assert!(paths(&levels, 1).is_empty(), "L1 未清除 —— 下一轮还会选中它");
        assert_eq!(paths(&levels, 0), vec![alive], "同层存在的文件不能被误伤");
    }

    #[test]
    fn no_op_when_every_file_is_present() {
        // 负向对照:文件都在时必须**一个都不删**,并返回空 —— 调用方据此决定
        // 报 error 还是 warn。摘过头会真丢数据,比不摘更严重。
        let dir = tempfile::tempdir().unwrap();
        let mut task = Vec::new();
        let mut want = Vec::new();
        for n in 0..3 {
            let p = dir.path().join(format!("{}.sst", n));
            std::fs::write(&p, b"x").unwrap();
            let p = p.to_string_lossy().to_string();
            task.push(info(&p, 0));
            want.push(p);
        }
        want.sort();

        let mut levels: HashMap<usize, Vec<SSTableInfo>> = HashMap::new();
        levels.insert(0, task.clone());

        let dropped = drop_missing_sstables(&mut levels, &task);
        assert!(dropped.is_empty(), "文件都在,不该摘任何条目");
        assert_eq!(paths(&levels, 0), want, "索引必须原样不动");
    }

    #[test]
    fn second_call_is_idempotent_and_stops_the_retry_loop() {
        // 这条直指原故障:第一次调用后该条目必须**再也选不中**。
        // 若索引没真清干净,第二次仍会报同一个 missing —— 那就是 2089 次的成因。
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("g.sst").to_string_lossy().to_string();
        let mut levels: HashMap<usize, Vec<SSTableInfo>> = HashMap::new();
        levels.insert(0, vec![info(&gone, 0)]);

        let first = drop_missing_sstables(&mut levels, &[info(&gone, 0)]);
        assert_eq!(first.len(), 1);
        assert!(paths(&levels, 0).is_empty());

        // 索引已空 —— 即便再拿旧任务来调,索引里也没有可摘的了
        let second = drop_missing_sstables(&mut levels, &[info(&gone, 0)]);
        assert_eq!(second.len(), 1, "返回值仍报告它不存在(调用方据此打 warn)");
        assert!(paths(&levels, 0).is_empty(), "索引仍为空,不会再被调度");
    }
}
