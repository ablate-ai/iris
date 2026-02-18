//! redb 持久化层
//!
//! 使用 redb 数据库进行长期存储

use anyhow::Result;
use common::proto::MetricsRequest;
use redb::{Database, ReadableTable, TableDefinition};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

/// 表定义: metrics
/// Key: "agent_id\0timestamp" (字符串，使用 \0 分隔)
/// Value: 序列化后的 MetricsRequest
const METRICS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("metrics");

/// 表定义: agent_latest
/// Key: agent_id
/// Value: 最新时间戳 (i64 序列化)
const AGENT_LATEST_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("agent_latest");

/// 进程内递增序号，用于避免同毫秒 key 冲突
static KEY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// 持久化存储
#[derive(Clone)]
pub struct PersistStorage {
    /// redb 数据库
    db: Arc<Database>,
}

impl PersistStorage {
    /// 创建新的持久化存储
    ///
    /// # Errors
    ///
    /// 如果数据库创建/打开失败，返回错误
    pub fn new(db_path: &str) -> Result<Self> {
        let path = Path::new(db_path);

        // 如果父目录不存在，创建它
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // 尝试创建或打开数据库
        let db = if path.exists() {
            info!("Opening existing redb database at {}", db_path);
            Database::open(path)?
        } else {
            info!("Creating new redb database at {}", db_path);
            Database::create(path)?
        };

        // 初始化表结构
        Self::init_tables(&db)?;

        Ok(Self { db: Arc::new(db) })
    }

    /// 初始化数据库表
    fn init_tables(db: &Database) -> Result<()> {
        let write_txn = db.begin_write()?;
        {
            // 打开或创建 metrics 表
            let _ = write_txn.open_table(METRICS_TABLE)?;
            // 打开或创建 agent_latest 表
            let _ = write_txn.open_table(AGENT_LATEST_TABLE)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// 生成复合键: "agent_id\0timestamp\0nonce"
    /// timestamp 固定 20 位用于排序；nonce 避免同毫秒覆盖
    fn make_key(agent_id: &str, timestamp: i64) -> String {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let seq = KEY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        format!(
            "{}\0{:020}\0{:032x}{:016x}",
            agent_id, timestamp, now_nanos, seq
        )
    }

    /// 解析复合键，返回 (agent_id, timestamp)
    fn parse_key(key: &str) -> Option<(&str, i64)> {
        // 新格式: agent_id\0timestamp\0nonce
        if let Some((agent_id, rest)) = key.split_once('\0') {
            let ts_str = rest.split('\0').next().unwrap_or(rest);
            let timestamp = ts_str.parse::<i64>().ok()?;
            return Some((agent_id, timestamp));
        }

        // 兼容旧格式: agent_id:timestamp（按最后一个冒号分割）
        if let Some((agent_id, ts_str)) = key.rsplit_once(':') {
            let timestamp = ts_str.parse::<i64>().ok()?;
            return Some((agent_id, timestamp));
        }

        None
    }

    /// 为查询构造 key 范围的起始和结束边界
    /// 返回 (start_key, end_key)
    fn make_key_range(agent_id: &str) -> (String, String) {
        // 起始: "agent_id\0"
        let start = format!("{}\0", agent_id);
        // 结束: 在最大 timestamp 后追加最大 Unicode 字符，覆盖带 nonce 的 key
        let end = format!("{}\0{:020}\u{10ffff}", agent_id, i64::MAX);
        (start, end)
    }

    /// 批量写入指标数据
    pub async fn flush_batch(&self, metrics: &[MetricsRequest]) -> Result<()> {
        if metrics.is_empty() {
            return Ok(());
        }

        let db = self.db.clone();
        let metrics = metrics.to_vec();

        // 在 blocking task 中执行，因为 redb 操作是同步的
        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;

            {
                let mut metrics_table = write_txn.open_table(METRICS_TABLE)?;
                let mut latest_table = write_txn.open_table(AGENT_LATEST_TABLE)?;

                for m in &metrics {
                    // 序列化 MetricsRequest
                    let bytes = bincode::serialize(&m)?;

                    // 写入 metrics 表
                    let key = Self::make_key(&m.agent_id, m.timestamp);
                    metrics_table.insert(key.as_str(), bytes.as_slice())?;

                    // 更新 agent_latest 表（只在时间戳更新时写入）
                    let should_update = match latest_table.get(m.agent_id.as_str())? {
                        Some(existing) => {
                            let arr = existing.value();
                            if arr.len() == 8 {
                                let existing_ts = i64::from_be_bytes([
                                    arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
                                ]);
                                m.timestamp > existing_ts
                            } else {
                                true
                            }
                        }
                        None => true,
                    };

                    if should_update {
                        let timestamp_bytes = m.timestamp.to_be_bytes();
                        latest_table.insert(m.agent_id.as_str(), timestamp_bytes.as_slice())?;
                    }
                }
            }

            write_txn.commit()?;
            debug!("Flushed {} metrics to redb", metrics.len());
            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }

    /// 获取指定 Agent 的最新指标
    pub async fn get_latest_metrics(&self, agent_id: &str) -> Result<Option<MetricsRequest>> {
        let db = self.db.clone();
        let agent_id = agent_id.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(METRICS_TABLE)?;
            let (start_prefix, end_prefix) = Self::make_key_range(&agent_id);

            let mut latest: Option<MetricsRequest> = None;

            let iter = table.range(start_prefix.as_str()..end_prefix.as_str())?;
            for item in iter {
                let (key, value) = item?;
                let key_str = key.value();
                if let Some((id, ts)) = Self::parse_key(key_str) {
                    if id == agent_id {
                        let metrics: MetricsRequest = bincode::deserialize(value.value())?;
                        if latest.as_ref().map(|m| m.timestamp).unwrap_or(i64::MIN) <= ts {
                            latest = Some(metrics);
                        }
                    }
                }
            }

            // 兼容旧格式 key（agent_id:timestamp）
            let all_iter = table.iter()?;
            for item in all_iter {
                let (key, value) = item?;
                let key_str = key.value();
                if key_str.contains('\0') {
                    continue;
                }
                if let Some((id, ts)) = Self::parse_key(key_str) {
                    if id == agent_id {
                        let metrics: MetricsRequest = bincode::deserialize(value.value())?;
                        if latest.as_ref().map(|m| m.timestamp).unwrap_or(i64::MIN) <= ts {
                            latest = Some(metrics);
                        }
                    }
                }
            }

            Ok::<Option<MetricsRequest>, anyhow::Error>(latest)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }

    /// 获取指定 Agent 最新 limit 条指标
    pub async fn query_latest_by_agent(
        &self,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<MetricsRequest>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let db = self.db.clone();
        let agent_id = agent_id.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(METRICS_TABLE)?;
            let (start_prefix, end_prefix) = Self::make_key_range(&agent_id);

            let mut results = Vec::new();
            let iter = table.range(start_prefix.as_str()..end_prefix.as_str())?;
            for item in iter {
                let (key, value) = item?;
                let key_str = key.value();
                if let Some((id, _)) = Self::parse_key(key_str) {
                    if id == agent_id {
                        let metrics: MetricsRequest = bincode::deserialize(value.value())?;
                        results.push(metrics);
                    }
                }
            }

            // 兼容旧格式 key（agent_id:timestamp）
            let all_iter = table.iter()?;
            for item in all_iter {
                let (key, value) = item?;
                let key_str = key.value();
                if key_str.contains('\0') {
                    continue;
                }
                if let Some((id, _)) = Self::parse_key(key_str) {
                    if id == agent_id {
                        let metrics: MetricsRequest = bincode::deserialize(value.value())?;
                        results.push(metrics);
                    }
                }
            }

            results.sort_by_key(|m| m.timestamp);
            if results.len() > limit {
                Ok::<Vec<MetricsRequest>, anyhow::Error>(results[results.len() - limit..].to_vec())
            } else {
                Ok::<Vec<MetricsRequest>, anyhow::Error>(results)
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }

    /// 获取所有 agent_id 列表
    pub async fn get_all_agent_ids(&self) -> Result<Vec<String>> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(AGENT_LATEST_TABLE)?;

            let mut agent_ids = Vec::new();
            let iter = table.iter()?;
            for item in iter {
                let (key, _) = item?;
                agent_ids.push(key.value().to_string());
            }

            debug!("获取到 {} 个 agent_id", agent_ids.len());
            Ok::<Vec<String>, anyhow::Error>(agent_ids)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }

    /// 删除指定 agent 超过保留数量的旧记录，返回删除数量
    ///
    /// 为避免内存占用过大，分批处理删除操作
    pub async fn delete_old_records(&self, agent_id: &str, keep_count: usize) -> Result<usize> {
        let db = self.db.clone();
        let agent_id = agent_id.to_string();

        tokio::task::spawn_blocking(move || {
            // 先读取该 agent 的所有 key
            let mut records: Vec<(i64, String)> = {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(METRICS_TABLE)?;

                let (start_prefix, end_prefix) = Self::make_key_range(&agent_id);

                let iter = table.range(start_prefix.as_str()..end_prefix.as_str())?;
                let mut keys = Vec::new();
                for item in iter {
                    let (key, _) = item?;
                    let key_str = key.value().to_string();
                    if let Some((id, ts)) = Self::parse_key(&key_str) {
                        if id == agent_id {
                            keys.push((ts, key_str));
                        }
                    }
                }
                keys
            };

            // 兼容旧格式 key（agent_id:timestamp）
            {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(METRICS_TABLE)?;
                let iter = table.iter()?;
                for item in iter {
                    let (key, _) = item?;
                    let key_str = key.value().to_string();
                    if key_str.contains('\0') {
                        continue;
                    }
                    if let Some((id, ts)) = Self::parse_key(&key_str) {
                        if id == agent_id {
                            records.push((ts, key_str));
                        }
                    }
                }
            }

            records.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

            let total = records.len();
            if total <= keep_count {
                debug!(
                    "Agent {} has {} records, keeping {} records, no deletion needed",
                    agent_id, total, keep_count
                );
                return Ok(0);
            }

            let delete_count = total - keep_count;
            let keys_to_delete: Vec<String> = records[..delete_count]
                .iter()
                .map(|(_, k)| k.clone())
                .collect();
            let latest_after = if delete_count < records.len() {
                Some(records[records.len() - 1].0)
            } else {
                None
            };

            // 分批删除，每批最多 10000 条，避免单次事务过大
            const BATCH_SIZE: usize = 10000;
            let mut total_deleted = 0;

            for chunk in keys_to_delete.chunks(BATCH_SIZE) {
                let write_txn = db.begin_write()?;
                {
                    let mut table = write_txn.open_table(METRICS_TABLE)?;
                    for key in chunk {
                        table.remove(key.as_str())?;
                    }
                }
                write_txn.commit()?;
                total_deleted += chunk.len();
            }

            // 同步更新 agent_latest 索引，避免陈旧数据
            let write_txn = db.begin_write()?;
            {
                let mut latest_table = write_txn.open_table(AGENT_LATEST_TABLE)?;
                if let Some(ts) = latest_after {
                    let ts_bytes = ts.to_be_bytes();
                    latest_table.insert(agent_id.as_str(), ts_bytes.as_slice())?;
                } else {
                    latest_table.remove(agent_id.as_str())?;
                }
            }
            write_txn.commit()?;

            info!(
                "Agent {} deleted {} old records, keeping {} records",
                agent_id, total_deleted, keep_count
            );
            Ok::<usize, anyhow::Error>(total_deleted)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }

    /// 删除指定时间之前的所有记录，返回删除数量
    ///
    /// 为避免内存占用过大和提升性能，按 agent 分批处理
    pub async fn delete_before_timestamp(&self, before_ts: i64) -> Result<usize> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            // 先获取所有 agent_id
            let agent_ids: Vec<String> = {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(AGENT_LATEST_TABLE)?;
                let mut ids = Vec::new();
                let iter = table.iter()?;
                for item in iter {
                    let (key, _) = item?;
                    ids.push(key.value().to_string());
                }
                ids
            };

            let mut total_deleted = 0;

            // 对每个 agent 分别处理，避免一次性遍历整个表
            for agent_id in agent_ids {
                // 收集该 agent 需要删除的 key
                let (keys_to_delete, latest_remaining_ts): (Vec<String>, Option<i64>) = {
                    let read_txn = db.begin_read()?;
                    let table = read_txn.open_table(METRICS_TABLE)?;

                    let (start_prefix, end_prefix) = Self::make_key_range(&agent_id);
                    let mut keys = Vec::new();
                    let mut latest_ts: Option<i64> = None;
                    let iter = table.range(start_prefix.as_str()..end_prefix.as_str())?;

                    for item in iter {
                        let (key, _) = item?;
                        let key_str = key.value();
                        if let Some((_, ts)) = Self::parse_key(key_str) {
                            if ts < before_ts {
                                keys.push(key_str.to_string());
                            } else if latest_ts.map(|v| ts > v).unwrap_or(true) {
                                latest_ts = Some(ts);
                            }
                        }
                    }
                    (keys, latest_ts)
                };

                // 兼容旧格式 key（agent_id:timestamp）
                let mut keys_to_delete = keys_to_delete;
                let mut latest_remaining_ts = latest_remaining_ts;
                {
                    let read_txn = db.begin_read()?;
                    let table = read_txn.open_table(METRICS_TABLE)?;
                    let iter = table.iter()?;
                    for item in iter {
                        let (key, _) = item?;
                        let key_str = key.value();
                        if key_str.contains('\0') {
                            continue;
                        }
                        if let Some((id, ts)) = Self::parse_key(key_str) {
                            if id == agent_id {
                                if ts < before_ts {
                                    keys_to_delete.push(key_str.to_string());
                                } else if latest_remaining_ts.map(|v| ts > v).unwrap_or(true) {
                                    latest_remaining_ts = Some(ts);
                                }
                            }
                        }
                    }
                }

                // 分批删除，每批最多 10000 条
                const BATCH_SIZE: usize = 10000;
                for chunk in keys_to_delete.chunks(BATCH_SIZE) {
                    let write_txn = db.begin_write()?;
                    {
                        let mut table = write_txn.open_table(METRICS_TABLE)?;
                        for key in chunk {
                            table.remove(key.as_str())?;
                        }
                    }
                    write_txn.commit()?;
                    total_deleted += chunk.len();
                }

                // 同步更新 agent_latest 索引
                let write_txn = db.begin_write()?;
                {
                    let mut latest_table = write_txn.open_table(AGENT_LATEST_TABLE)?;
                    if let Some(ts) = latest_remaining_ts {
                        let ts_bytes = ts.to_be_bytes();
                        latest_table.insert(agent_id.as_str(), ts_bytes.as_slice())?;
                    } else {
                        latest_table.remove(agent_id.as_str())?;
                    }
                }
                write_txn.commit()?;
            }

            if total_deleted > 0 {
                info!("删除了 {} 条早于 {} 的记录", total_deleted, before_ts);
            } else {
                debug!("没有早于 {} 的记录需要删除", before_ts);
            }

            Ok::<usize, anyhow::Error>(total_deleted)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::proto::*;

    trait PersistStorageTestExt {
        async fn query_by_agent(
            &self,
            agent_id: &str,
            start_ts: i64,
            end_ts: i64,
        ) -> Result<Vec<MetricsRequest>>;
        async fn get_agent_latest_timestamp(&self, agent_id: &str) -> Result<Option<i64>>;
    }

    impl PersistStorageTestExt for PersistStorage {
        async fn query_by_agent(
            &self,
            agent_id: &str,
            start_ts: i64,
            end_ts: i64,
        ) -> Result<Vec<MetricsRequest>> {
            let mut results = self.query_latest_by_agent(agent_id, usize::MAX).await?;
            results.retain(|m| m.timestamp >= start_ts && m.timestamp <= end_ts);
            Ok(results)
        }

        async fn get_agent_latest_timestamp(&self, agent_id: &str) -> Result<Option<i64>> {
            Ok(self
                .get_latest_metrics(agent_id)
                .await?
                .map(|m| m.timestamp))
        }
    }

    /// 创建完整的测试指标数据
    fn create_test_metrics(agent_id: &str, timestamp: i64) -> MetricsRequest {
        MetricsRequest {
            agent_id: agent_id.to_string(),
            timestamp,
            hostname: "test-host".to_string(),
            system: Some(SystemMetrics {
                cpu: Some(CpuMetrics {
                    usage_percent: 50.0,
                    core_count: 4,
                    per_core: vec![25.0, 50.0, 75.0, 100.0],
                    load_avg_1: 1.0,
                    load_avg_5: 0.8,
                    load_avg_15: 0.5,
                }),
                memory: Some(MemoryMetrics {
                    total: 16_000_000_000,
                    used: 8_000_000_000,
                    available: 8_000_000_000,
                    usage_percent: 50.0,
                    swap_total: 2_000_000_000,
                    swap_used: 0,
                }),
                disks: vec![DiskMetrics {
                    mount_point: "/".to_string(),
                    device: "/dev/sda1".to_string(),
                    total: 500_000_000_000,
                    used: 250_000_000_000,
                    available: 250_000_000_000,
                    usage_percent: 50.0,
                    read_bytes: 1_000_000,
                    write_bytes: 500_000,
                }],
                network: Some(NetworkMetrics {
                    bytes_sent: 1_000_000_000,
                    bytes_recv: 5_000_000_000,
                    packets_sent: 100_000,
                    packets_recv: 500_000,
                    errors_in: 0,
                    errors_out: 0,
                }),
                system_info: Some(SystemInfo {
                    os_name: "Linux".to_string(),
                    os_version: "5.15.0".to_string(),
                    kernel_version: "5.15.0-generic".to_string(),
                    arch: "x86_64".to_string(),
                    uptime: 86400,
                    cpu_model: "Test CPU".to_string(),
                    cpu_frequency: 3000.0,
                    hostname: "test-host".to_string(),
                }),
                agent_metrics: Some(AgentMetrics {
                    cpu_usage: 5.0,
                    memory_usage: 50_000_000,
                    collection_time_ms: 100,
                    uptime_seconds: 3600,
                    metrics_sent: 1000,
                    errors_count: 0,
                }),
            }),
        }
    }

    #[tokio::test]
    async fn test_persist_batch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        let metrics = vec![
            create_test_metrics("agent-1", 1000),
            create_test_metrics("agent-1", 2000),
            create_test_metrics("agent-2", 1500),
        ];

        storage.flush_batch(&metrics).await.unwrap();

        // 验证查询
        let result = storage.query_by_agent("agent-1", 0, 9999).await.unwrap();
        assert_eq!(result.len(), 2);

        let latest_ts = storage
            .get_agent_latest_timestamp("agent-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest_ts, 2000);
    }

    #[tokio::test]
    async fn test_persist_query_range() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        let metrics = vec![
            create_test_metrics("agent-1", 1000),
            create_test_metrics("agent-1", 2000),
            create_test_metrics("agent-1", 3000),
        ];

        storage.flush_batch(&metrics).await.unwrap();

        // 查询范围: 1500-2500，应该只有 2000
        let result = storage.query_by_agent("agent-1", 1500, 2500).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].timestamp, 2000);
    }

    #[tokio::test]
    async fn test_persist_empty_batch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();
        storage.flush_batch(&[]).await.unwrap(); // 不应该 panic
    }

    #[tokio::test]
    async fn test_persist_update_existing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 写入初始数据
        storage
            .flush_batch(&[create_test_metrics("agent-1", 1000)])
            .await
            .unwrap();

        // 同一毫秒重复写入应保留多条（避免覆盖丢数据）
        storage
            .flush_batch(&[create_test_metrics("agent-1", 1000)])
            .await
            .unwrap();

        let result = storage.query_by_agent("agent-1", 0, 9999).await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_persist_query_nonexistent_agent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        let result = storage
            .query_by_agent("nonexistent", 0, 9999)
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_persist_get_latest_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        let result = storage
            .get_agent_latest_timestamp("nonexistent")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_persist_multiple_batches() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 写入多个批次
        for i in 0..10 {
            storage
                .flush_batch(&[create_test_metrics("agent-1", i * 1000)])
                .await
                .unwrap();
        }

        let result = storage.query_by_agent("agent-1", 0, 99999).await.unwrap();
        assert_eq!(result.len(), 10);

        let latest = storage
            .get_agent_latest_timestamp("agent-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest, 9000);
    }

    #[tokio::test]
    async fn test_persist_key_format() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 测试包含特殊字符的 agent_id
        let special_ids = ["agent-1", "agent_2", "agent.3", "agent:4"];

        for (i, id) in special_ids.iter().enumerate() {
            storage
                .flush_batch(&[create_test_metrics(id, (i as i64) * 1000)])
                .await
                .unwrap();
        }

        // 验证每个 agent 都能正确查询
        for (i, id) in special_ids.iter().enumerate() {
            let result = storage.query_by_agent(id, 0, 99999).await.unwrap();
            assert_eq!(result.len(), 1, "Failed for agent_id: {}", id);
            assert_eq!(result[0].timestamp, (i as i64) * 1000);
        }
    }

    #[tokio::test]
    async fn test_persist_large_data() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 写入大量数据
        let metrics: Vec<_> = (0..1000)
            .map(|i| create_test_metrics("agent-1", i as i64))
            .collect();

        storage.flush_batch(&metrics).await.unwrap();

        let result = storage.query_by_agent("agent-1", 0, 2000).await.unwrap();
        assert_eq!(result.len(), 1000);
    }

    #[tokio::test]
    async fn test_persist_concurrent_writes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();
        let mut handles = vec![];

        // 并发写入
        for i in 0..10 {
            let storage_clone = storage.clone();
            let handle = tokio::spawn(async move {
                let metrics: Vec<_> = (0..100)
                    .map(|j| create_test_metrics(&format!("agent-{}", i), j as i64))
                    .collect();
                storage_clone.flush_batch(&metrics).await.unwrap();
            });
            handles.push(handle);
        }

        // 等待所有写入完成
        for handle in handles {
            handle.await.unwrap();
        }

        // 验证数据
        for i in 0..10 {
            let result = storage
                .query_by_agent(&format!("agent-{}", i), 0, 200)
                .await
                .unwrap();
            assert_eq!(result.len(), 100, "Failed for agent-{}", i);
        }
    }

    #[tokio::test]
    async fn test_persist_concurrent_reads() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 先写入数据
        let metrics: Vec<_> = (0..100)
            .map(|i| create_test_metrics("agent-1", i as i64))
            .collect();
        storage.flush_batch(&metrics).await.unwrap();

        // 并发读取
        let mut handles = vec![];
        for _ in 0..10 {
            let storage_clone = storage.clone();
            let handle = tokio::spawn(async move {
                let result = storage_clone
                    .query_by_agent("agent-1", 0, 100)
                    .await
                    .unwrap();
                result.len()
            });
            handles.push(handle);
        }

        // 验证所有读取都返回正确的结果
        for handle in handles {
            let count = handle.await.unwrap();
            assert_eq!(count, 100);
        }
    }

    #[tokio::test]
    async fn test_persist_timestamp_ordering() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 乱序写入
        let timestamps = vec![5000, 1000, 3000, 4000, 2000];
        for ts in &timestamps {
            storage
                .flush_batch(&[create_test_metrics("agent-1", *ts)])
                .await
                .unwrap();
        }

        let result = storage.query_by_agent("agent-1", 0, 10000).await.unwrap();

        // 验证所有数据都被存储
        assert_eq!(result.len(), 5);

        // 验证时间戳正确
        let mut result_timestamps: Vec<i64> = result.iter().map(|m| m.timestamp).collect();
        result_timestamps.sort_unstable();
        assert_eq!(result_timestamps, vec![1000, 2000, 3000, 4000, 5000]);
    }

    #[tokio::test]
    async fn test_persist_reopen_database() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        // 写入数据
        {
            let storage = PersistStorage::new(&db_path).unwrap();
            storage
                .flush_batch(&[create_test_metrics("agent-1", 1000)])
                .await
                .unwrap();
        }

        // 重新打开数据库
        let storage = PersistStorage::new(&db_path).unwrap();
        let result = storage.query_by_agent("agent-1", 0, 9999).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].timestamp, 1000);
    }

    #[tokio::test]
    async fn test_get_all_agent_ids() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 空数据库
        let ids = storage.get_all_agent_ids().await.unwrap();
        assert!(ids.is_empty());

        // 写入多个 agent 的数据
        let metrics = vec![
            create_test_metrics("agent-a", 1000),
            create_test_metrics("agent-b", 2000),
            create_test_metrics("agent-c", 3000),
        ];
        storage.flush_batch(&metrics).await.unwrap();

        let mut ids = storage.get_all_agent_ids().await.unwrap();
        ids.sort();
        assert_eq!(ids, vec!["agent-a", "agent-b", "agent-c"]);
    }

    #[tokio::test]
    async fn test_delete_old_records() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 写入 5 条记录
        let metrics: Vec<_> = (1..=5)
            .map(|i| create_test_metrics("agent-1", i * 1000))
            .collect();
        storage.flush_batch(&metrics).await.unwrap();

        // 保留 2 条，应删除 3 条
        let deleted = storage.delete_old_records("agent-1", 2).await.unwrap();
        assert_eq!(deleted, 3);

        // 验证剩余的是最新的 2 条
        let remaining = storage.query_by_agent("agent-1", 0, 99999).await.unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].timestamp, 4000);
        assert_eq!(remaining[1].timestamp, 5000);
    }

    #[tokio::test]
    async fn test_delete_old_records_keep_more_than_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 写入 3 条记录
        let metrics: Vec<_> = (1..=3)
            .map(|i| create_test_metrics("agent-1", i * 1000))
            .collect();
        storage.flush_batch(&metrics).await.unwrap();

        // 保留 10 条（超过实际数量），不应删除任何记录
        let deleted = storage.delete_old_records("agent-1", 10).await.unwrap();
        assert_eq!(deleted, 0);

        let remaining = storage.query_by_agent("agent-1", 0, 99999).await.unwrap();
        assert_eq!(remaining.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_before_timestamp() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        // 写入多个 agent 的数据
        let metrics = vec![
            create_test_metrics("agent-1", 1000),
            create_test_metrics("agent-1", 2000),
            create_test_metrics("agent-1", 3000),
            create_test_metrics("agent-2", 1500),
            create_test_metrics("agent-2", 2500),
        ];
        storage.flush_batch(&metrics).await.unwrap();

        // 删除时间戳 < 2000 的记录（agent-1:1000, agent-2:1500）
        let deleted = storage.delete_before_timestamp(2000).await.unwrap();
        assert_eq!(deleted, 2);

        // 验证 agent-1 剩余记录
        let r1 = storage.query_by_agent("agent-1", 0, 99999).await.unwrap();
        assert_eq!(r1.len(), 2);
        assert_eq!(r1[0].timestamp, 2000);
        assert_eq!(r1[1].timestamp, 3000);

        // 验证 agent-2 剩余记录
        let r2 = storage.query_by_agent("agent-2", 0, 99999).await.unwrap();
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].timestamp, 2500);
    }

    #[tokio::test]
    async fn test_delete_before_timestamp_no_match() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();

        let storage = PersistStorage::new(&db_path).unwrap();

        let metrics = vec![create_test_metrics("agent-1", 5000)];
        storage.flush_batch(&metrics).await.unwrap();

        // 所有记录都在 1000 之后，不应删除
        let deleted = storage.delete_before_timestamp(1000).await.unwrap();
        assert_eq!(deleted, 0);

        let remaining = storage.query_by_agent("agent-1", 0, 99999).await.unwrap();
        assert_eq!(remaining.len(), 1);
    }
}
