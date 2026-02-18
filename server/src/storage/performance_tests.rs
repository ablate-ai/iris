//! Storage 性能测试和并发测试

use super::*;
use common::proto::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
            system_info: None,
            agent_metrics: None,
        }),
    }
}

async fn wait_history_len(storage: &Storage, agent_id: &str, expected: usize, timeout_ms: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let history = storage.get_agent_history(agent_id, expected).await;
        if history.len() >= expected {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Timed out waiting history for {agent_id}: got {}, expected {expected}",
                history.len()
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn test_performance_single_write() {
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

    let metrics = create_test_metrics("agent-1", 1000);

    let start = Instant::now();
    storage.save_metrics(&metrics).await;
    let elapsed = start.elapsed();

    // 单次写入应该在 1ms 内完成（只是写入缓存和发送到通道）
    assert!(
        elapsed.as_millis() < 10,
        "Single write took too long: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_performance_batch_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        batch_size: 100,
        batch_timeout: Duration::from_millis(100),
        ..Default::default()
    });

    let count = 1000;
    let start = Instant::now();

    for i in 0..count {
        let metrics = create_test_metrics("agent-1", i);
        storage.save_metrics(&metrics).await;
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    // 批量写入应该达到很高的吞吐量
    println!(
        "Batch write throughput: {:.2} ops/sec, total time: {:?}",
        throughput, elapsed
    );

    assert!(throughput > 1000.0, "Throughput too low: {:.2}", throughput);

    // 等待批量写入完成
    tokio::time::sleep(Duration::from_millis(200)).await;
}

#[tokio::test]
async fn test_performance_cache_read() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        ..Default::default()
    });

    // 预填充数据
    for i in 0..100 {
        let metrics = create_test_metrics("agent-1", i);
        storage.save_metrics(&metrics).await;
    }

    let count = 10000;
    let start = Instant::now();

    for _ in 0..count {
        let _ = storage.get_agent_latest("agent-1").await;
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!(
        "Cache read throughput: {:.2} ops/sec, total time: {:?}",
        throughput, elapsed
    );

    // 缓存读取应该非常快
    assert!(
        throughput > 10000.0,
        "Cache read throughput too low: {:.2}",
        throughput
    );
}

#[tokio::test]
async fn test_concurrent_single_agent_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Arc::new(Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 50,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 1000,
        ..Default::default()
    }));

    let num_tasks = 10;
    let writes_per_task = 100;
    let mut handles = vec![];

    let start = Instant::now();

    // 多个任务并发写入同一个 agent
    for task_id in 0..num_tasks {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move {
            for i in 0..writes_per_task {
                let timestamp = task_id * writes_per_task + i;
                let metrics = create_test_metrics("agent-1", timestamp as i64);
                storage_clone.save_metrics(&metrics).await;
            }
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
        handle.await.unwrap();
    }

    let elapsed = start.elapsed();
    let total_writes = num_tasks * writes_per_task;
    let throughput = total_writes as f64 / elapsed.as_secs_f64();

    println!(
        "Concurrent writes (single agent): {} writes in {:?} ({:.2} ops/sec)",
        total_writes, elapsed, throughput
    );

    wait_history_len(&storage, "agent-1", total_writes, 5_000).await;

    // 持久化模式下，历史查询会返回完整结果，而不受缓存 100 条限制
    let history = storage.get_agent_history("agent-1", 2000).await;
    assert_eq!(
        history.len(),
        total_writes,
        "History should include persisted data"
    );
}

#[tokio::test]
async fn test_concurrent_multiple_agents_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Arc::new(Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 50,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 1000,
        ..Default::default()
    }));

    let num_agents = 10;
    let writes_per_agent = 50;
    let mut handles = vec![];

    let start = Instant::now();

    // 每个任务写入一个不同的 agent
    for agent_id in 0..num_agents {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move {
            for i in 0..writes_per_agent {
                let metrics = create_test_metrics(&format!("agent-{}", agent_id), i);
                storage_clone.save_metrics(&metrics).await;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let elapsed = start.elapsed();
    let total_writes = num_agents * writes_per_agent;
    let throughput = total_writes as f64 / elapsed.as_secs_f64();

    println!(
        "Concurrent writes (multiple agents): {} writes in {:?} ({:.2} ops/sec)",
        total_writes, elapsed, throughput
    );

    // 等待批量写入完成
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 验证所有 agent 都有数据
    let agents = storage.get_all_agents().await;
    assert_eq!(agents.len(), num_agents as usize);

    for agent_id in 0..num_agents {
        let agent_id_str = format!("agent-{}", agent_id);
        let history = storage.get_agent_history(&agent_id_str, 100).await;
        assert_eq!(history.len(), writes_per_agent as usize);
    }
}

#[tokio::test]
async fn test_concurrent_reads_and_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Arc::new(Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 50,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 1000,
        ..Default::default()
    }));

    let mut handles = vec![];

    // 写入任务
    for i in 0..5 {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move {
            for j in 0..100 {
                let metrics = create_test_metrics("agent-1", i * 100 + j);
                storage_clone.save_metrics(&metrics).await;
            }
        });
        handles.push(handle);
    }

    // 读取任务
    for _ in 0..5 {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let _ = storage_clone.get_agent_latest("agent-1").await;
                let _ = storage_clone.get_agent_history("agent-1", 10).await;
            }
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
        handle.await.unwrap();
    }

    // 验证数据一致性
    let latest = storage.get_agent_latest("agent-1").await;
    assert!(latest.is_some());
}

#[tokio::test]
async fn test_concurrent_get_all_agents() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Arc::new(Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        ..Default::default()
    }));

    // 预填充数据
    for i in 0..20 {
        let metrics = create_test_metrics(&format!("agent-{}", i), i);
        storage.save_metrics(&metrics).await;
    }

    let num_tasks = 10;
    let mut handles = vec![];

    // 并发获取所有 agent
    for _ in 0..num_tasks {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move { storage_clone.get_all_agents().await });
        handles.push(handle);
    }

    // 验证所有读取都返回正确的数量
    for handle in handles {
        let agents = handle.await.unwrap();
        assert_eq!(agents.len(), 20);
    }
}

#[tokio::test]
async fn test_memory_leak_simulation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 50,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 1000,
        ..Default::default()
    });

    // 写入大量数据
    for i in 0..1000 {
        let metrics = create_test_metrics("agent-1", i);
        storage.save_metrics(&metrics).await;
    }

    wait_history_len(&storage, "agent-1", 1000, 5_000).await;

    // 持久化模式下，历史查询会返回完整结果
    let history = storage.get_agent_history("agent-1", 2000).await;
    assert_eq!(history.len(), 1000, "History should include persisted data");
}

#[tokio::test]
async fn test_stress_multiple_agents_with_different_rates() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Arc::new(Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 50,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 1000,
        ..Default::default()
    }));

    let mut handles = vec![];

    // 不同 agent 以不同速率写入
    for agent_id in 0..5 {
        let storage_clone = storage.clone();
        let write_count = 50 + agent_id * 50; // 不同的写入量
        let handle = tokio::spawn(async move {
            for i in 0..write_count {
                let metrics = create_test_metrics(&format!("agent-{}", agent_id), i);
                storage_clone.save_metrics(&metrics).await;
                // agent-0 最快，agent-4 最慢
                tokio::time::sleep(Duration::from_micros(100 * (agent_id + 1) as u64)).await;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // 等待批量写入完成
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 验证每个 agent 都有正确数量的数据
    for agent_id in 0..5 {
        let expected = 50 + agent_id * 50;
        let agent_id_str = format!("agent-{}", agent_id);
        let history = storage.get_agent_history(&agent_id_str, 200).await;
        assert_eq!(
            history.len(),
            expected.min(200),
            "Agent {} has wrong count",
            agent_id
        );
    }
}

#[tokio::test]
async fn test_large_payload_performance() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir
        .path()
        .join("test.db")
        .to_str()
        .unwrap()
        .to_string();

    let storage = Storage::with_config(StorageConfig {
        db_path: Some(db_path),
        cache_size_per_agent: 100,
        batch_size: 50,
        batch_timeout: Duration::from_millis(100),
        channel_capacity: 1000,
        ..Default::default()
    });

    // 创建带有大量数据的指标
    let mut large_metrics = create_test_metrics("agent-1", 1000);
    if let Some(system) = &mut large_metrics.system {
        // 添加大量磁盘数据，扩大单条 payload 体积
        system.disks = (0..110)
            .map(|i| DiskMetrics {
                mount_point: format!("/mnt/{}", i),
                device: format!("/dev/sd{}", i),
                total: 1_000_000_000_000,
                used: 500_000_000_000,
                available: 500_000_000_000,
                usage_percent: 50.0,
                read_bytes: i * 1_000_000,
                write_bytes: i * 500_000,
            })
            .collect();
    }

    let count = 100;
    let start = Instant::now();

    for i in 0..count {
        let mut metrics = large_metrics.clone();
        metrics.timestamp = i;
        storage.save_metrics(&metrics).await;
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!(
        "Large payload throughput: {:.2} ops/sec, avg time: {:?}",
        throughput,
        elapsed / count as u32
    );

    // 等待批量写入完成
    tokio::time::sleep(Duration::from_millis(200)).await;
}
