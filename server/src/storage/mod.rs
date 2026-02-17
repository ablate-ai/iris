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
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// 批量写入配置
pub const BATCH_SIZE: usize = 50;
pub const BATCH_TIMEOUT: Duration = Duration::from_secs(5);
pub const CHANNEL_CAPACITY: usize = 1000;

/// 写入请求（可选等待持久化完成）
#[derive(Debug)]
struct WriteRequest {
    metrics: MetricsRequest,
    ack: Option<oneshot::Sender<Result<()>>>,
}

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
    /// 每个 Agent 保留的最大记录数
    pub max_records_per_agent: usize,
    /// 数据保留天数
    pub retention_days: u64,
    /// 清理任务执行间隔（小时）
    pub cleanup_interval_hours: u64,
    /// 是否启用清理任务
    pub enable_cleanup: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: None, // 默认仅内存模式
            cache_size_per_agent: 100,
            batch_size: BATCH_SIZE,
            batch_timeout: BATCH_TIMEOUT,
            channel_capacity: CHANNEL_CAPACITY,
            // 保留约 7 天数据（1秒1次上报：7 × 86400 = 604,800 条）
            max_records_per_agent: 604_800,
            retention_days: 0,         // 禁用时间清理，仅按数量限制
            cleanup_interval_hours: 6, // 每 6 小时清理一次
            enable_cleanup: true,
        }
    }
}

/// Storage - 异步批量写入存储
///
/// 数据流:
/// 1. 收到 MetricsRequest
/// 2. 立即更新内存缓存 (供快速查询)
/// 3. 如果启用持久化：发送到写入队列 (阻塞等待)
/// 4. 后台任务累积到 50 条或 5 秒后批量写入 redb
#[derive(Clone)]
pub struct Storage {
    /// 内存缓存
    cache: Arc<cache::Cache>,
    /// 写入通道 sender（仅持久化模式），包装在 Arc<RwLock> 中以支持 shutdown 时关闭
    write_tx: Option<Arc<RwLock<Option<mpsc::Sender<WriteRequest>>>>>,
    /// 后台任务句柄
    writer_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    /// 运行状态
    running: Arc<RwLock<bool>>,
    /// 是否启用持久化
    persist_enabled: bool,
    /// 持久化存储引用（用于清理任务）
    persist: Option<Arc<PersistStorage>>,
    /// 清理任务句柄
    cleanup_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    /// 清理任务停止标志
    cleanup_running: Option<Arc<std::sync::atomic::AtomicBool>>,
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
        let (write_tx, writer_handle, persist_enabled, persist, cleanup_handle, cleanup_running) =
            if let Some(db_path) = &config.db_path {
                match PersistStorage::new(db_path) {
                    Ok(persist) => {
                        let persist = Arc::new(persist);
                        let (tx, rx) = mpsc::channel(config.channel_capacity);

                        // 启动后台批量写入任务
                        let running_clone = running.clone();
                        let persist_clone = persist.clone();
                        let handle = tokio::spawn(async move {
                            Self::batch_writer_task(
                                rx,
                                persist_clone,
                                config.batch_size,
                                config.batch_timeout,
                                running_clone,
                            )
                            .await;
                        });

                        // 启动清理任务（如果启用）
                        let (cleanup_handle, cleanup_running) = if config.enable_cleanup {
                            let cleanup_task =
                                cleanup::CleanupTask::new(config.clone(), persist.clone());
                            let cleanup_running = cleanup_task.running_flag();
                            let handle = tokio::spawn(async move {
                                cleanup_task.run().await;
                            });
                            (Some(Arc::new(handle)), Some(cleanup_running))
                        } else {
                            (None, None)
                        };

                        info!(
                            db_path = %db_path,
                            cache_size = config.cache_size_per_agent,
                            batch_size = config.batch_size,
                            enable_cleanup = config.enable_cleanup,
                            "Storage initialized with persistence"
                        );

                        (
                            Some(Arc::new(RwLock::new(Some(tx)))),
                            Some(Arc::new(handle)),
                            true,
                            Some(persist),
                            cleanup_handle,
                            cleanup_running,
                        )
                    }
                    Err(e) => {
                        error!(
                            db_path = %db_path,
                            error = %e,
                            "Failed to initialize persistence, fallback to memory-only mode"
                        );
                        (None, None, false, None, None, None)
                    }
                }
            } else {
                info!(
                    cache_size = config.cache_size_per_agent,
                    "Storage initialized in memory-only mode"
                );

                (None, None, false, None, None, None)
            };

        if !persist_enabled {
            info!("Persistence is disabled");
        }

        Self {
            cache,
            write_tx,
            writer_handle,
            running,
            persist_enabled,
            persist,
            cleanup_handle,
            cleanup_running,
        }
    }

    /// 是否已启用持久化
    pub fn is_persist_enabled(&self) -> bool {
        self.persist_enabled
    }

    async fn enqueue_metrics(&self, metrics: &MetricsRequest, wait_persist: bool) -> Result<()> {
        let tx_opt = if let Some(tx_lock) = &self.write_tx {
            tx_lock.read().await.clone()
        } else {
            None
        };

        let Some(tx) = tx_opt else {
            if self.persist_enabled {
                return Err(anyhow::anyhow!("persistence queue is closed"));
            }
            return Ok(());
        };

        let (ack_tx, ack_rx) = if wait_persist {
            let (tx, rx) = oneshot::channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        tx.send(WriteRequest {
            metrics: metrics.clone(),
            ack: ack_tx,
        })
        .await
        .map_err(|e| anyhow::anyhow!("failed to send metrics to queue: {}", e))?;

        if let Some(rx) = ack_rx {
            rx.await
                .map_err(|e| anyhow::anyhow!("failed waiting persistence ack: {}", e))??;
        }

        Ok(())
    }

    /// 保存指标数据（仅保证写入缓存，持久化为异步排队）
    pub async fn save_metrics(&self, metrics: &MetricsRequest) {
        self.cache.update(metrics.clone()).await;

        if let Err(e) = self.enqueue_metrics(metrics, false).await {
            error!(
                agent_id = %metrics.agent_id,
                error = %e,
                "Failed to enqueue metrics for persistence"
            );
        }

        debug!(
            agent_id = %metrics.agent_id,
            timestamp = metrics.timestamp,
            persist = self.persist_enabled,
            "Metrics saved to cache{}",
            if self.persist_enabled { " and queued for persistence" } else { "" }
        );
    }

    /// 保存指标数据并等待持久化落盘完成
    pub async fn save_metrics_sync(&self, metrics: &MetricsRequest) -> Result<()> {
        self.cache.update(metrics.clone()).await;

        self.enqueue_metrics(metrics, true).await?;

        debug!(
            agent_id = %metrics.agent_id,
            timestamp = metrics.timestamp,
            persist = self.persist_enabled,
            "Metrics saved to cache and persisted"
        );
        Ok(())
    }

    /// 获取所有 Agent ID
    pub async fn get_all_agents(&self) -> Vec<String> {
        let mut agent_set: HashSet<String> =
            self.cache.get_all_agents().await.into_iter().collect();

        if let Some(persist) = &self.persist {
            match persist.get_all_agent_ids().await {
                Ok(ids) => {
                    for id in ids {
                        agent_set.insert(id);
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to load agent ids from persistence");
                }
            }
        }

        let mut agents: Vec<String> = agent_set.into_iter().collect();
        agents.sort();
        agents
    }

    /// 获取指定 Agent 的最新指标
    pub async fn get_agent_latest(&self, agent_id: &str) -> Option<MetricsRequest> {
        let cache_latest = self.cache.get_latest(agent_id).await;
        if cache_latest.is_some() {
            return cache_latest;
        }

        if let Some(persist) = &self.persist {
            match persist.get_latest_metrics(agent_id).await {
                Ok(v) => v,
                Err(e) => {
                    error!(agent_id = %agent_id, error = %e, "Failed to load latest metrics from persistence");
                    None
                }
            }
        } else {
            None
        }
    }

    /// 获取指定 Agent 的历史指标
    pub async fn get_agent_history(&self, agent_id: &str, limit: usize) -> Vec<MetricsRequest> {
        if limit == 0 {
            return Vec::new();
        }

        let cache_history = self.cache.get_history(agent_id, limit).await;
        // 仅内存模式下，缓存是唯一数据源
        if self.persist.is_none() {
            return cache_history;
        }

        // 持久化模式下：优先返回完整历史，避免缓存命中导致历史截断
        if let Some(persist) = &self.persist {
            if cache_history.len() >= limit {
                return cache_history;
            }

            match persist.query_latest_by_agent(agent_id, limit).await {
                Ok(mut persisted) => {
                    if persisted.is_empty() {
                        return cache_history;
                    }

                    // 合并缓存和持久化结果，按时间戳+内容去重
                    persisted.extend(cache_history);
                    persisted.sort_by_key(|m| m.timestamp);
                    persisted.dedup_by(|a, b| a.timestamp == b.timestamp && a == b);

                    if persisted.len() > limit {
                        persisted[persisted.len() - limit..].to_vec()
                    } else {
                        persisted
                    }
                }
                Err(e) => {
                    error!(agent_id = %agent_id, error = %e, "Failed to load history from persistence");
                    cache_history
                }
            }
        } else {
            cache_history
        }
    }

    /// 优雅关闭
    ///
    /// 等待队列中的数据全部写入完成
    pub async fn shutdown(&self) -> Result<()> {
        info!("Storage shutdown initiated");

        // 如果启用了持久化，关闭写入通道并等待任务完成
        if self.persist_enabled {
            // 停止清理任务
            if let (Some(cleanup_running), Some(cleanup_handle)) =
                (&self.cleanup_running, &self.cleanup_handle)
            {
                info!("Stopping cleanup task...");
                cleanup_running.store(false, std::sync::atomic::Ordering::SeqCst);

                // 等待清理任务完成（最多 5 秒）
                // 使用 AbortHandle 来检查和 abort，但不直接 await（因为这是 &self）
                let cleanup_abort = cleanup_handle.abort_handle();
                match tokio::time::timeout(Duration::from_secs(5), async {
                    // 轮询检查任务是否完成
                    loop {
                        if cleanup_handle.is_finished() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await
                {
                    Ok(_) => info!("Cleanup task stopped successfully"),
                    Err(_) => {
                        warn!("Cleanup task timeout, aborting...");
                        cleanup_abort.abort();
                    }
                }
            }

            // 关闭发送端，让接收端知道不会再有新数据
            if let Some(tx_lock) = &self.write_tx {
                let tx = tx_lock.write().await.take();
                drop(tx); // 显式 drop，关闭通道
                info!("Write channel closed");
            }

            // 标记为不再运行（让批量写入任务作为备用退出机制）
            *self.running.write().await = false;

            // 等待批量写入任务完成（最多 10 秒）
            if let Some(writer_handle) = &self.writer_handle {
                let writer_abort = writer_handle.abort_handle();
                match tokio::time::timeout(Duration::from_secs(10), async {
                    // 轮询检查任务是否完成
                    loop {
                        if writer_handle.is_finished() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await
                {
                    Ok(_) => info!("Batch writer task stopped successfully"),
                    Err(_) => {
                        warn!("Batch writer task timeout, aborting...");
                        writer_abort.abort();
                    }
                }
            }
        }

        info!("Storage shutdown complete");
        Ok(())
    }

    /// 后台批量写入任务
    async fn batch_writer_task(
        mut rx: mpsc::Receiver<WriteRequest>,
        persist: Arc<PersistStorage>,
        batch_size: usize,
        timeout: Duration,
        running: Arc<RwLock<bool>>,
    ) {
        let mut buffer = Vec::with_capacity(batch_size);
        let mut pending_acks: Vec<oneshot::Sender<Result<()>>> = Vec::with_capacity(batch_size);
        let mut interval = tokio::time::interval(timeout);

        info!("Batch writer task started");

        loop {
            tokio::select! {
                // 接收新数据
                result = rx.recv() => {
                    match result {
                        Some(req) => {
                            buffer.push(req.metrics);
                            if let Some(ack) = req.ack {
                                pending_acks.push(ack);
                            }

                            // 达到批量大小，立即写入
                            if buffer.len() >= batch_size {
                                Self::flush_buffer(&persist, &mut buffer, &mut pending_acks, "batch size reached").await;
                            }
                        }
                        None => {
                            // 通道关闭，退出循环
                            info!("Write channel closed, exiting batch writer");
                            break;
                        }
                    }
                }
                // 超时触发
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        Self::flush_buffer(&persist, &mut buffer, &mut pending_acks, "timeout").await;
                    }

                    // 检查是否应该继续运行（备用退出机制）
                    if !*running.read().await {
                        info!("Shutdown signal received, exiting batch writer");
                        break;
                    }
                }
            }
        }

        // 刷新剩余数据
        if !buffer.is_empty() {
            info!(
                "Flushing remaining {} metrics before shutdown",
                buffer.len()
            );
            if !Self::flush_buffer(&persist, &mut buffer, &mut pending_acks, "shutdown").await {
                for ack in pending_acks.drain(..) {
                    let _ = ack.send(Err(anyhow::anyhow!(
                        "batch writer stopped before data was persisted"
                    )));
                }
            }
        }

        info!("Batch writer task stopped");
    }

    async fn flush_buffer(
        persist: &Arc<PersistStorage>,
        buffer: &mut Vec<MetricsRequest>,
        pending_acks: &mut Vec<oneshot::Sender<Result<()>>>,
        reason: &str,
    ) -> bool {
        if buffer.is_empty() {
            return true;
        }

        match persist.flush_batch(buffer).await {
            Ok(_) => {
                debug!("Flushed {} metrics ({})", buffer.len(), reason);
                for ack in pending_acks.drain(..) {
                    let _ = ack.send(Ok(()));
                }
                buffer.clear();
                true
            }
            Err(e) => {
                error!("Failed to flush batch ({}): {}", reason, e);
                false
            }
        }
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
