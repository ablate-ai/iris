//! Storage 集成测试
//!
//! 测试完整的写入和查询流程

use super::*;
use common::proto::*;
use std::time::Duration;

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
            disks: vec![],
            network: Some(NetworkMetrics {
                bytes_sent: 0,
                bytes_recv: 0,
                packets_sent: 0,
                packets_recv: 0,
                errors_in: 0,
                errors_out: 0,
            }),
            processes: vec![],
            system_info: None,
            agent_metrics: None,
        }),
    }
}

#[tokio::test]
async fn test_storage_write_and_read() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 10,
        batch_size: 5,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 写入数据
    let metrics = create_test_metrics("agent-1", 1000);
    storage.save_metrics(&metrics).await;

    // 立即从缓存读取
    let latest = storage.get_agent_latest("agent-1").await;
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().timestamp, 1000);

    // 等待批量写入完成
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 验证 agent 列表
    let agents = storage.get_all_agents().await;
    assert_eq!(agents.len(), 1);
    assert!(agents.contains(&"agent-1".to_string()));
}

#[tokio::test]
async fn test_storage_multiple_agents() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 10,
        batch_size: 5,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 写入多个 agent 的数据
    for i in 1..=5 {
        let metrics = create_test_metrics(&format!("agent-{}", i), i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // 等待批量写入
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 验证所有 agent 都在列表中
    let agents = storage.get_all_agents().await;
    assert_eq!(agents.len(), 5);

    // 验证每个 agent 的最新数据
    for i in 1..=5 {
        let agent_id = format!("agent-{}", i);
        let latest = storage.get_agent_latest(&agent_id).await;
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().timestamp, i * 1000);
    }
}

#[tokio::test]
async fn test_storage_history() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 10,
        batch_size: 5,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 写入多条历史数据
    for i in 1..=10 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // 等待批量写入
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 获取历史数据
    let history = storage.get_agent_history("agent-1", 5).await;
    assert_eq!(history.len(), 5);

    // 应该是最新的 5 条
    assert_eq!(history[0].timestamp, 6000);
    assert_eq!(history[4].timestamp, 10000);
}

#[tokio::test]
async fn test_storage_history_fallback_to_persistence_when_cache_insufficient() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 5,
        batch_size: 10,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    for i in 1..=20 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    tokio::time::sleep(Duration::from_millis(300)).await;

    // limit 大于缓存容量，应从持久化补齐到完整结果
    let history = storage.get_agent_history("agent-1", 20).await;
    assert_eq!(history.len(), 20);
    assert_eq!(history[0].timestamp, 1000);
    assert_eq!(history[19].timestamp, 20000);
}

#[tokio::test]
async fn test_storage_batch_write() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    // 设置较小的 batch_size 来测试批量写入
    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 3,
        batch_timeout: Duration::from_secs(10), // 较长的超时
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 写入 3 条数据，正好触发批量写入
    for i in 1..=3 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // 等待批量写入完成
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 验证数据在缓存中
    let history = storage.get_agent_history("agent-1", 10).await;
    assert_eq!(history.len(), 3);
}

#[tokio::test]
async fn test_storage_timeout_flush() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    // 设置较短的超时时间
    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 100, // 较大的 batch
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 只写入 1 条数据，不足以触发批量写入
    let metrics = create_test_metrics("agent-1", 1000);
    storage.save_metrics(&metrics).await;

    // 等待超时触发刷新
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 数据应该在缓存中
    let latest = storage.get_agent_latest("agent-1").await;
    assert!(latest.is_some());
}

#[tokio::test]
async fn test_storage_cache_limit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    // 设置较小的缓存大小
    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 5,
        batch_size: 10,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 写入超过缓存大小的数据
    for i in 1..=10 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // 等待批量写入
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 历史查询应从持久化补齐，返回完整 10 条
    let history = storage.get_agent_history("agent-1", 20).await;
    assert_eq!(history.len(), 10);
    assert_eq!(history[0].timestamp, 1000);
    assert_eq!(history[9].timestamp, 10000);
}

#[tokio::test]
async fn test_storage_multiple_agents_cache_isolation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 3,
        batch_size: 10,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // agent-1 写入 5 条
    for i in 1..=5 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // agent-2 写入 2 条
    for i in 1..=2 {
        let metrics = create_test_metrics("agent-2", i * 2000);
        storage.save_metrics(&metrics).await;
    }

    // 等待批量写入
    tokio::time::sleep(Duration::from_millis(200)).await;

    // agent-1 历史查询应返回完整 5 条
    let history1 = storage.get_agent_history("agent-1", 10).await;
    assert_eq!(history1.len(), 5);

    // agent-2 应该保留全部 2 条
    let history2 = storage.get_agent_history("agent-2", 10).await;
    assert_eq!(history2.len(), 2);
}

#[tokio::test]
async fn test_storage_shutdown() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 10,
        batch_size: 100,
        batch_timeout: Duration::from_secs(10),
        channel_capacity: 100,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 写入数据
    for i in 1..=5 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // 关闭存储（应该刷新缓冲区）
    storage.shutdown().await.unwrap();

    // 等待关闭完成
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_storage_channel_full() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 1000, // 很大的 batch，不会立即刷新
        batch_timeout: Duration::from_secs(60),
        channel_capacity: 5, // 很小的通道容量
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 快速写入超过通道容量的数据
    for i in 0..10 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // 所有数据都应该在缓存中（因为写入是 try_send，失败只记录警告）
    let latest = storage.get_agent_latest("agent-1").await;
    assert!(latest.is_some());
}

#[tokio::test]
async fn test_storage_empty_query() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        ..Default::default()
    });

    // 查询不存在的 agent
    assert!(storage.get_agent_latest("nonexistent").await.is_none());
    assert!(storage
        .get_agent_history("nonexistent", 10)
        .await
        .is_empty());
    assert!(storage.get_all_agents().await.is_empty());
}

#[tokio::test]
async fn test_storage_persistence_across_restarts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    // 第一次写入
    {
        let config = StorageConfig {
            db_path: Some(db_path.clone()),
            cache_size_per_agent: 10,
            batch_size: 2,
            batch_timeout: Duration::from_millis(50),
            channel_capacity: 100,
            ..Default::default()
        };

        let storage = Storage::with_config(config);
        for i in 1..=5 {
            let metrics = create_test_metrics("agent-1", i * 1000);
            storage.save_metrics(&metrics).await;
        }
        storage.shutdown().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // "重启" - 新的 Storage 实例
    {
        let config = StorageConfig {
            db_path: Some(db_path),
            cache_size_per_agent: 10,
            batch_size: 2,
            batch_timeout: Duration::from_millis(50),
            channel_capacity: 100,
            ..Default::default()
        };

        let storage = Storage::with_config(config);

        // 数据应该持久化在数据库中（缓存为空，但数据在磁盘上）
        // 注意：当前实现中，缓存从磁盘重新加载的功能还未实现
        // 这里主要测试数据库可以正常打开和写入
        let metrics = create_test_metrics("agent-2", 5000);
        storage.save_metrics(&metrics).await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        // 新数据应该在缓存中
        let latest = storage.get_agent_latest("agent-2").await;
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().timestamp, 5000);
    }
}

#[tokio::test]
async fn test_storage_high_frequency_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 50,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 1000,
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 高频写入 200 条数据
    for i in 0..200 {
        let metrics = create_test_metrics("agent-1", i);
        storage.save_metrics(&metrics).await;
    }

    // 等待所有批量写入完成
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 历史查询应返回持久化中的完整 200 条
    let history = storage.get_agent_history("agent-1", 200).await;
    assert_eq!(history.len(), 200);
}

#[tokio::test]
async fn test_storage_cleanup_disabled() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    // 禁用清理任务
    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 10,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        max_records_per_agent: 5, // 小的限制
        retention_days: 1,        // 1 天保留期
        enable_cleanup: false,    // 禁用清理
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 写入超过限制的数据
    for i in 1..=10 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // 等待批量写入
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 等待一段时间，确认清理任务没有运行（没有 panic）
    tokio::time::sleep(Duration::from_millis(500)).await;

    storage.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_storage_cleanup_shutdown() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    // 启用清理任务，设置较短的时间间隔以便快速测试
    let config = StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 10,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 100,
        max_records_per_agent: 10000,
        retention_days: 30,
        cleanup_interval_hours: 1, // 1 小时间隔
        enable_cleanup: true,      // 启用清理
        ..Default::default()
    };

    let storage = Storage::with_config(config);

    // 写入一些数据
    for i in 1..=5 {
        let metrics = create_test_metrics("agent-1", i * 1000);
        storage.save_metrics(&metrics).await;
    }

    // 等待批量写入
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 关闭存储（应该优雅停止清理任务）
    storage.shutdown().await.unwrap();

    // 等待关闭完成
    tokio::time::sleep(Duration::from_millis(200)).await;
}
