//! Storage 模块 - 指标数据存储
//!
//! 架构:
//! - Cache (cache.rs): 内存缓存层，每个 Agent 保留最新 100 条数据
//! - Persist (persist.rs): redb 持久化层，长期存储
//! - 本模块 (mod.rs): 异步批量写入队列，整合缓存和持久化

pub mod cache;
pub mod cleanup;
pub mod persist;

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod performance_tests;

use anyhow::Result;
use common::proto::MetricsRequest;
use persist::PersistStorage;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// 批量写入配置
pub const BATCH_SIZE: usize = 50;
pub const BATCH_TIMEOUT: Duration = Duration::from_secs(5);
pub const CHANNEL_CAPACITY: usize = 1000;

/// Storage 配置
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// 数据库文件路径（None 表示仅内存模式）
    pub db_path: Option<String>,
    /// 每个 Agent 在内存中缓存的最大条数
    pub cache_size_per_agent: usize,
    /// 批量写入大小
    pub batch_size: usize,
    /// 批量写入超时
    pub batch_timeout: Duration,
    /// 写入通道容量
    pub channel_capacity: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: None, // 默认仅内存模式
            cache_size_per_agent: 100,
            batch_size: BATCH_SIZE,
            batch_timeout: BATCH_TIMEOUT,
            channel_capacity: CHANNEL_CAPACITY,
        }
    }
}

/// Storage - 异步批量写入存储
///
/// 数据流:
/// 1. 收到 MetricsRequest
/// 2. 立即更新内存缓存 (供快速查询)
/// 3. 如果启用持久化：发送到写入队列 (非阻塞)
/// 4. 后台任务累积到 50 条或 5 秒后批量写入 redb
#[derive(Clone)]
pub struct Storage {
    /// 内存缓存
    cache: Arc<cache::Cache>,
    /// 写入通道 sender（仅持久化模式）
    write_tx: Option<mpsc::Sender<MetricsRequest>>,
    /// 运行状态
    running: Arc<RwLock<bool>>,
    /// 是否启用持久化
    persist_enabled: bool,
}

impl Storage {
    /// 创建新的 Storage 实例（默认仅内存模式）
    pub fn new() -> Self {
        Self::with_config(StorageConfig::default())
    }

    /// 使用自定义配置创建 Storage
    pub fn with_config(config: StorageConfig) -> Self {
        let cache = Arc::new(cache::Cache::new(config.cache_size_per_agent));
        let running = Arc::new(RwLock::new(true));

        // 根据配置决定是否启用持久化
        let (write_tx, persist_enabled) = if let Some(db_path) = &config.db_path {
            let persist = PersistStorage::new(db_path);
            let (tx, rx) = mpsc::channel(config.channel_capacity);

            // 启动后台批量写入任务
            let cache_clone = cache.clone();
            let running_clone = running.clone();
            tokio::spawn(async move {
                Self::batch_writer_task(
                    rx,
                    persist,
                    cache_clone,
                    config.batch_size,
                    config.batch_timeout,
                    running_clone,
                )
                .await;
            });

            info!(
                db_path = %db_path,
                cache_size = config.cache_size_per_agent,
                batch_size = config.batch_size,
                "Storage initialized with persistence"
            );

            (Some(tx), true)
        } else {
            info!(
                cache_size = config.cache_size_per_agent,
                "Storage initialized in memory-only mode"
            );

            (None, false)
        };

        Self {
            cache,
            write_tx,
            running,
            persist_enabled,
        }
    }

    /// 保存指标数据
    ///
    /// 流程:
    /// 1. 立即更新内存缓存
    /// 2. 如果启用持久化：异步发送到持久化队列
    pub async fn save_metrics(&self, metrics: &MetricsRequest) {
        // 1. 立即更新缓存
        self.cache.update(metrics.clone()).await;

        // 2. 如果启用持久化，发送到持久化队列 (非阻塞)
        if let Some(tx) = &self.write_tx {
            if let Err(_) = tx.try_send(metrics.clone()) {
                // 通道满时记录警告，但不阻塞
                warn!(
                    agent_id = %metrics.agent_id,
                    "Write channel full, metrics may be lost"
                );
            }
        }

        debug!(
            agent_id = %metrics.agent_id,
            timestamp = metrics.timestamp,
            persist = self.persist_enabled,
            "Metrics saved to cache{}",
            if self.persist_enabled { " and queued for persistence" } else { "" }
        );
    }

    /// 获取所有 Agent ID
    pub async fn get_all_agents(&self) -> Vec<String> {
        self.cache.get_all_agents().await
    }

    /// 获取指定 Agent 的最新指标
    pub async fn get_agent_latest(&self, agent_id: &str) -> Option<MetricsRequest> {
        self.cache.get_latest(agent_id).await
    }

    /// 获取指定 Agent 的历史指标
    pub async fn get_agent_history(&self, agent_id: &str, limit: usize) -> Vec<MetricsRequest> {
        self.cache.get_history(agent_id, limit).await
    }

    /// 优雅关闭
    ///
    /// 等待队列中的数据全部写入完成
    pub async fn shutdown(&self) -> Result<()> {
        info!("Storage shutdown initiated");

        // 标记为不再运行
        *self.running.write().await = false;

        // 如果启用了持久化，等待写入通道关闭
        if self.persist_enabled {
            info!("Waiting for persistence queue to flush...");
            // 等待一小段时间让后台任务完成
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        info!("Storage shutdown complete");
        Ok(())
    }

    /// 后台批量写入任务
    async fn batch_writer_task(
        mut rx: mpsc::Receiver<MetricsRequest>,
        persist: PersistStorage,
        _cache: Arc<cache::Cache>,
        batch_size: usize,
        timeout: Duration,
        running: Arc<RwLock<bool>>,
    ) {
        let mut buffer = Vec::with_capacity(batch_size);
        let mut interval = tokio::time::interval(timeout);

        info!("Batch writer task started");

        loop {
            tokio::select! {
                // 接收新数据
                Some(metrics) = rx.recv() => {
                    buffer.push(metrics);

                    // 达到批量大小，立即写入
                    if buffer.len() >= batch_size {
                        if let Err(e) = persist.flush_batch(&buffer).await {
                            error!("Failed to flush batch: {}", e);
                        } else {
                            debug!("Flushed {} metrics (batch size reached)", buffer.len());
                        }
                        buffer.clear();
                    }
                }
                // 超时触发
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        if let Err(e) = persist.flush_batch(&buffer).await {
                            error!("Failed to flush batch on timeout: {}", e);
                        } else {
                            debug!("Flushed {} metrics (timeout)", buffer.len());
                        }
                        buffer.clear();
                    }
                }
                // 检查运行状态
                else => {
                    // 如果通道关闭且缓冲区为空，退出
                    if rx.is_closed() && buffer.is_empty() {
                        break;
                    }
                }
            }

            // 检查是否应该继续运行
            if !*running.read().await {
                // 刷新剩余数据
                if !buffer.is_empty() {
                    info!("Flushing remaining {} metrics before shutdown", buffer.len());
                    if let Err(e) = persist.flush_batch(&buffer).await {
                        error!("Failed to flush final batch: {}", e);
                    }
                }
                break;
            }
        }

        info!("Batch writer task stopped");
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.db_path, None); // 默认仅内存模式
        assert_eq!(config.cache_size_per_agent, 100);
        assert_eq!(config.batch_size, BATCH_SIZE);
    }

    #[test]
    fn test_config_with_persistence() {
        let config = StorageConfig {
            db_path: Some("test.db".to_string()),
            ..Default::default()
        };
        assert_eq!(config.db_path, Some("test.db".to_string()));
    }
}
