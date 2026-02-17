//! 数据清理任务
//!
//! 定期清理过期的指标数据：
//! - 每个 Agent 保留最近 max_records_per_agent 条
//! - 删除超过 retention_days 天的旧数据

use crate::storage::StorageConfig;

/// 清理任务
pub struct CleanupTask {
    _config: StorageConfig,
}

impl CleanupTask {
    /// 创建清理任务
    pub fn new(config: StorageConfig) -> Self {
        Self { _config: config }
    }

    /// 启动定期清理（后台任务）
    pub async fn run(&self) {
        // TODO: 启动定时器，定期执行清理逻辑
        // - 每小时扫描一次
        // - 每个 Agent 保留最近 max_records_per_agent 条
        // - 删除超过 retention_days 天的数据
    }
}
