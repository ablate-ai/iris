//! redb 持久化层
//!
//! 使用 redb 数据库进行长期存储

use anyhow::Result;
use common::proto::MetricsRequest;
use redb::{Database, ReadableTable, TableDefinition};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

/// 表定义: metrics
/// Key: "agent_id:timestamp" (字符串)
/// Value: 序列化后的 MetricsRequest
const METRICS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("metrics");

/// 表定义: agent_latest
/// Key: agent_id
/// Value: 最新时间戳 (i64 序列化)
const AGENT_LATEST_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("agent_latest");

/// 持久化存储
pub struct PersistStorage {
    /// redb 数据库
    db: Arc<Database>,
}

impl PersistStorage {
    /// 创建新的持久化存储
    pub fn new(db_path: &str) -> Self {
        let path = Path::new(db_path);

        // 创建数据库（如果不存在）
        let db = match Database::create(path) {
            Ok(db) => {
                info!("Created/Open redb database at {}", db_path);
                db
            }
            Err(e) => {
                // 如果已存在，尝试打开
                info!("Database exists, opening: {}", e);
                Database::open(path).expect("Failed to open database")
            }
        };

        // 初始化表结构
        Self::init_tables(&db);

        Self { db: Arc::new(db) }
    }

    /// 初始化数据库表
    fn init_tables(db: &Database) {
        let write_txn = db.begin_write().expect("Failed to begin write");
        {
            // 打开或创建 metrics 表
            let _ = write_txn.open_table(METRICS_TABLE);
            // 打开或创建 agent_latest 表
            let _ = write_txn.open_table(AGENT_LATEST_TABLE);
        }
        write_txn.commit().expect("Failed to commit init");
    }

    /// 生成复合键: "agent_id:timestamp"
    fn make_key(agent_id: &str, timestamp: i64) -> String {
        format!("{}:{}", agent_id, timestamp)
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

                    // 更新 agent_latest 表
                    let timestamp_bytes = m.timestamp.to_be_bytes();
                    latest_table.insert(m.agent_id.as_str(), timestamp_bytes.as_slice())?;
                }
            }

            write_txn.commit()?;
            debug!("Flushed {} metrics to redb", metrics.len());
            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }

    /// 查询指定 Agent 在时间范围内的指标
    pub async fn query_by_agent(
        &self,
        agent_id: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<MetricsRequest>> {
        let db = self.db.clone();
        let agent_id = agent_id.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(METRICS_TABLE)?;

            let mut results = Vec::new();

            // 遍历所有条目，筛选匹配的
            let iter = table.iter()?;
            for item in iter {
                let (key, value) = item?;
                let key_str = key.value();

                // 解析 key: "agent_id:timestamp" (从右边分割最后一个冒号)
                // 这样 agent_id 可以包含冒号
                if let Some((id, ts_str)) = key_str.rsplit_once(':') {
                    if id == agent_id {
                        if let Ok(ts) = ts_str.parse::<i64>() {
                            if ts >= start_ts && ts <= end_ts {
                                let metrics: MetricsRequest = bincode::deserialize(value.value())?;
                                results.push(metrics);
                            }
                        }
                    }
                }
            }

            Ok::<Vec<MetricsRequest>, anyhow::Error>(results)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }

    /// 获取指定 Agent 的最新时间戳
    pub async fn get_agent_latest_timestamp(&self, agent_id: &str) -> Result<Option<i64>> {
        let db = self.db.clone();
        let agent_id = agent_id.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(AGENT_LATEST_TABLE)?;

            match table.get(agent_id.as_str())? {
                Some(bytes) => {
                    let arr = bytes.value();
                    if arr.len() == 8 {
                        let ts = i64::from_be_bytes([
                            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
                        ]);
                        Ok(Some(ts))
                    } else {
                        Ok(None)
                    }
                }
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::proto::*;

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
                processes: vec![
                    ProcessMetrics {
                        pid: 1234,
                        name: "test-process".to_string(),
                        cpu_usage: 10.0,
                        memory: 100_000_000,
                        status: "Running".to_string(),
                    },
                ],
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
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        let metrics = vec![
            create_test_metrics("agent-1", 1000),
            create_test_metrics("agent-1", 2000),
            create_test_metrics("agent-2", 1500),
        ];

        storage.flush_batch(&metrics).await.unwrap();

        // 验证查询
        let result = storage
            .query_by_agent("agent-1", 0, 9999)
            .await
            .unwrap();
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
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        let metrics = vec![
            create_test_metrics("agent-1", 1000),
            create_test_metrics("agent-1", 2000),
            create_test_metrics("agent-1", 3000),
        ];

        storage.flush_batch(&metrics).await.unwrap();

        // 查询范围: 1500-2500，应该只有 2000
        let result = storage
            .query_by_agent("agent-1", 1500, 2500)
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].timestamp, 2000);
    }

    #[tokio::test]
    async fn test_persist_empty_batch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);
        storage.flush_batch(&[]).await.unwrap(); // 不应该 panic
    }

    #[tokio::test]
    async fn test_persist_update_existing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        // 写入初始数据
        storage
            .flush_batch(&[create_test_metrics("agent-1", 1000)])
            .await
            .unwrap();

        // 更新同一 key 的数据（应该覆盖）
        storage
            .flush_batch(&[create_test_metrics("agent-1", 1000)])
            .await
            .unwrap();

        let result = storage
            .query_by_agent("agent-1", 0, 9999)
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_persist_query_nonexistent_agent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        let result = storage
            .query_by_agent("nonexistent", 0, 9999)
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_persist_get_latest_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        let result = storage
            .get_agent_latest_timestamp("nonexistent")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_persist_multiple_batches() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        // 写入多个批次
        for i in 0..10 {
            storage
                .flush_batch(&[create_test_metrics("agent-1", i * 1000)])
                .await
                .unwrap();
        }

        let result = storage
            .query_by_agent("agent-1", 0, 99999)
            .await
            .unwrap();
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
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        // 测试包含特殊字符的 agent_id
        let special_ids = vec!["agent-1", "agent_2", "agent.3", "agent:4"];

        for (i, id) in special_ids.iter().enumerate() {
            storage
                .flush_batch(&[create_test_metrics(id, (i as i64) * 1000)])
                .await
                .unwrap();
        }

        // 验证每个 agent 都能正确查询
        for (i, id) in special_ids.iter().enumerate() {
            let result = storage
                .query_by_agent(id, 0, 99999)
                .await
                .unwrap();
            assert_eq!(result.len(), 1, "Failed for agent_id: {}", id);
            assert_eq!(result[0].timestamp, (i as i64) * 1000);
        }
    }

    #[tokio::test]
    async fn test_persist_large_data() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        // 写入大量数据
        let metrics: Vec<_> = (0..1000)
            .map(|i| create_test_metrics("agent-1", i as i64))
            .collect();

        storage.flush_batch(&metrics).await.unwrap();

        let result = storage
            .query_by_agent("agent-1", 0, 2000)
            .await
            .unwrap();
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

        let storage = std::sync::Arc::new(PersistStorage::new(&db_path));
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
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = std::sync::Arc::new(PersistStorage::new(&db_path));

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
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let storage = PersistStorage::new(&db_path);

        // 乱序写入
        let timestamps = vec![5000, 1000, 3000, 4000, 2000];
        for ts in &timestamps {
            storage
                .flush_batch(&[create_test_metrics("agent-1", *ts)])
                .await
                .unwrap();
        }

        let result = storage
            .query_by_agent("agent-1", 0, 10000)
            .await
            .unwrap();

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
            let storage = PersistStorage::new(&db_path);
            storage
                .flush_batch(&[create_test_metrics("agent-1", 1000)])
                .await
                .unwrap();
        }

        // 重新打开数据库
        let storage = PersistStorage::new(&db_path);
        let result = storage
            .query_by_agent("agent-1", 0, 9999)
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].timestamp, 1000);
    }
}
