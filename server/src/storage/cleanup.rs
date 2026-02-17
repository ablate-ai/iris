//! 数据清理任务
//!
//! 定期清理过期的指标数据：
//! - 每个 Agent 保留最近 max_records_per_agent 条
//! - 删除超过 retention_days 天的旧数据

use crate::storage::persist::PersistStorage;
use crate::storage::StorageConfig;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

/// 清理任务
pub struct CleanupTask {
    config: StorageConfig,
    /// 持久化存储引用
    storage: Arc<PersistStorage>,
    /// 运行状态标志，用于优雅停止
    running: Arc<AtomicBool>,
}

impl CleanupTask {
    /// 创建清理任务
    pub fn new(config: StorageConfig, storage: Arc<PersistStorage>) -> Self {
        // 输入验证
        assert!(config.cleanup_interval_hours > 0, "cleanup_interval_hours must be positive");
        assert!(config.max_records_per_agent > 0, "max_records_per_agent must be positive");
        // retention_days 允许为 0（禁用时间清理）

        Self {
            config,
            storage,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    /// 获取 running 标志的克隆，用于外部控制停止
    pub fn running_flag(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    /// 启动定期清理（后台任务）
    pub async fn run(&self) {
        let interval = Duration::from_secs(self.config.cleanup_interval_hours * 3600);
        let mut ticker = tokio::time::interval(interval);
        // 创建一个独立的检查间隔，用于及时响应停止信号
        let mut check_interval = tokio::time::interval(Duration::from_secs(10));

        info!(
            interval_hours = self.config.cleanup_interval_hours,
            max_records_per_agent = self.config.max_records_per_agent,
            retention_days = self.config.retention_days,
            "Cleanup task started"
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // 检查是否应该继续运行
                    if !self.running.load(Ordering::SeqCst) {
                        info!("Cleanup task received stop signal, exiting");
                        break;
                    }

                    self.execute_cleanup().await;
                }
                // 定期检查停止信号（避免在长时间间隔中无法响应）
                _ = check_interval.tick() => {
                    if !self.running.load(Ordering::SeqCst) {
                        info!("Cleanup task received stop signal, exiting");
                        break;
                    }
                }
            }
        }

        info!("Cleanup task stopped");
    }

    /// 执行一次清理
    async fn execute_cleanup(&self) {
        info!("Starting data cleanup");

        // 获取所有 agent_id
        let agent_ids = match self.storage.get_all_agent_ids().await {
            Ok(ids) => ids,
            Err(e) => {
                error!("Failed to get agent_id list: {}", e);
                return;
            }
        };

        if agent_ids.is_empty() {
            info!("No agent data to clean");
            return;
        }

        let mut total_deleted_by_count = 0usize;
        let mut agents_cleaned = 0usize;

        // 1. 对每个 agent 执行数量限制清理
        for agent_id in &agent_ids {
            // 检查停止信号，避免长时间清理过程中无法响应
            if !self.running.load(Ordering::SeqCst) {
                warn!("Received stop signal during cleanup, exiting early");
                return;
            }

            match self
                .storage
                .delete_old_records(agent_id, self.config.max_records_per_agent)
                .await
            {
                Ok(deleted) => {
                    if deleted > 0 {
                        total_deleted_by_count += deleted;
                        agents_cleaned += 1;
                    }
                }
                Err(e) => {
                    error!(
                        agent_id = %agent_id,
                        error = %e,
                        "Failed to delete old records for agent"
                    );
                }
            }
        }

        // 2. 执行时间限制清理（仅当 retention_days > 0 时）
        let total_deleted_by_time = if self.config.retention_days > 0 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let cutoff_ts = now.saturating_sub(self.config.retention_days.saturating_mul(86400) as i64);
            match self.storage.delete_before_timestamp(cutoff_ts).await {
                Ok(deleted) => deleted,
                Err(e) => {
                    error!("Failed to delete expired records: {}", e);
                    0
                }
            }
        } else {
            0
        };

        info!(
            agents_total = agent_ids.len(),
            agents_cleaned = agents_cleaned,
            deleted_by_count = total_deleted_by_count,
            deleted_by_time = total_deleted_by_time,
            retention_days = self.config.retention_days,
            "Data cleanup completed"
        );
    }
}
