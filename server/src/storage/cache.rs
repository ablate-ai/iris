//! 内存缓存层
//!
//! 使用 HashMap + VecDeque 实现每个 Agent 的固定大小缓存

use common::proto::MetricsRequest;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 内存缓存 - 每个 Agent 保留最新 N 条数据
#[derive(Clone)]
pub struct Cache {
    /// 最大缓存条数 (每个 Agent)
    max_size: usize,
    /// agent_id -> 数据队列
    data: Arc<RwLock<HashMap<String, VecDeque<MetricsRequest>>>>,
}

impl Cache {
    /// 创建新的缓存
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 更新缓存
    pub async fn update(&self, metrics: MetricsRequest) {
        let agent_id = metrics.agent_id.clone();
        let mut data = self.data.write().await;

        let entry = data.entry(agent_id).or_insert_with(VecDeque::new);
        entry.push_back(metrics);

        // 超过最大条数时，移除最旧的数据
        while entry.len() > self.max_size {
            entry.pop_front();
        }
    }

    /// 获取所有 Agent ID
    pub async fn get_all_agents(&self) -> Vec<String> {
        let data = self.data.read().await;
        data.keys().cloned().collect()
    }

    /// 获取指定 Agent 的最新一条数据
    pub async fn get_latest(&self, agent_id: &str) -> Option<MetricsRequest> {
        let data = self.data.read().await;
        data.get(agent_id).and_then(|v| v.back().cloned())
    }

    /// 获取指定 Agent 的历史数据（最多 limit 条）
    pub async fn get_history(&self, agent_id: &str, limit: usize) -> Vec<MetricsRequest> {
        let data = self.data.read().await;
        if let Some(entry) = data.get(agent_id) {
            let len = entry.len();
            let start = if len > limit { len - limit } else { 0 };
            entry.range(start..).cloned().collect()
        } else {
            Vec::new()
        }
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
    async fn test_cache_update() {
        let cache = Cache::new(3);

        cache.update(create_test_metrics("agent-1", 1000)).await;
        cache.update(create_test_metrics("agent-1", 2000)).await;

        assert_eq!(cache.get_all_agents().await.len(), 1);
        assert_eq!(cache.get_latest("agent-1").await.unwrap().timestamp, 2000);
    }

    #[tokio::test]
    async fn test_cache_max_size() {
        let cache = Cache::new(3);

        cache.update(create_test_metrics("agent-1", 1000)).await;
        cache.update(create_test_metrics("agent-1", 2000)).await;
        cache.update(create_test_metrics("agent-1", 3000)).await;
        cache.update(create_test_metrics("agent-1", 4000)).await; // 超出

        let history = cache.get_history("agent-1", 10).await;
        assert_eq!(history.len(), 3);
        // 最旧的 (1000) 应该被移除
        assert_eq!(history[0].timestamp, 2000);
        assert_eq!(history[2].timestamp, 4000);
    }

    #[tokio::test]
    async fn test_get_history_limit() {
        let cache = Cache::new(100);

        for i in 1..=10 {
            cache.update(create_test_metrics("agent-1", i)).await;
        }

        let history = cache.get_history("agent-1", 5).await;
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].timestamp, 6); // 最后 5 条
        assert_eq!(history[4].timestamp, 10);
    }

    #[tokio::test]
    async fn test_multiple_agents() {
        let cache = Cache::new(10);

        cache.update(create_test_metrics("agent-1", 1000)).await;
        cache.update(create_test_metrics("agent-2", 2000)).await;
        cache.update(create_test_metrics("agent-3", 3000)).await;

        let agents = cache.get_all_agents().await;
        assert_eq!(agents.len(), 3);
        assert!(agents.contains(&"agent-1".to_string()));
        assert!(agents.contains(&"agent-2".to_string()));
        assert!(agents.contains(&"agent-3".to_string()));
    }

    #[tokio::test]
    async fn test_nonexistent_agent() {
        let cache = Cache::new(10);

        assert!(cache.get_latest("nonexistent").await.is_none());
        assert!(cache.get_history("nonexistent", 10).await.is_empty());
    }

    #[tokio::test]
    async fn test_cache_size_limit_per_agent() {
        let cache = Cache::new(5);

        // agent-1 写入 10 条，应该只保留最后 5 条
        for i in 1..=10 {
            cache.update(create_test_metrics("agent-1", i)).await;
        }

        // agent-2 写入 3 条
        for i in 1..=3 {
            cache.update(create_test_metrics("agent-2", i * 100)).await;
        }

        let history1 = cache.get_history("agent-1", 100).await;
        assert_eq!(history1.len(), 5);
        assert_eq!(history1[0].timestamp, 6);
        assert_eq!(history1[4].timestamp, 10);

        let history2 = cache.get_history("agent-2", 100).await;
        assert_eq!(history2.len(), 3);
    }

    #[tokio::test]
    async fn test_cache_update_existing_agent() {
        let cache = Cache::new(10);

        // 更新同一个 agent 的数据
        cache.update(create_test_metrics("agent-1", 1000)).await;
        cache.update(create_test_metrics("agent-1", 2000)).await;

        let latest = cache.get_latest("agent-1").await.unwrap();
        assert_eq!(latest.timestamp, 2000);

        let history = cache.get_history("agent-1", 10).await;
        assert_eq!(history.len(), 2);
    }

    #[tokio::test]
    async fn test_cache_lru_behavior() {
        let cache = Cache::new(3);

        // 写入 3 条
        cache.update(create_test_metrics("agent-1", 1)).await;
        cache.update(create_test_metrics("agent-1", 2)).await;
        cache.update(create_test_metrics("agent-1", 3)).await;

        // 写入第 4 条，第 1 条应该被移除
        cache.update(create_test_metrics("agent-1", 4)).await;

        let history = cache.get_history("agent-1", 10).await;
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].timestamp, 2);
        assert_eq!(history[2].timestamp, 4);
    }

    #[tokio::test]
    async fn test_cache_empty_history() {
        let cache = Cache::new(10);

        // 查询不存在的 agent
        let history = cache.get_history("nonexistent", 10).await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn test_cache_history_limit_exceeds_cached() {
        let cache = Cache::new(5);

        for i in 1..=5 {
            cache.update(create_test_metrics("agent-1", i)).await;
        }

        // 请求超过缓存数量的历史
        let history = cache.get_history("agent-1", 100).await;
        assert_eq!(history.len(), 5);
    }

    #[tokio::test]
    async fn test_cache_zero_limit() {
        let cache = Cache::new(100);

        for i in 1..=10 {
            cache.update(create_test_metrics("agent-1", i)).await;
        }

        // limit = 0 应该返回空
        let history = cache.get_history("agent-1", 0).await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn test_cache_concurrent_updates() {
        let cache = std::sync::Arc::new(Cache::new(100));
        let mut handles = vec![];

        // 10 个任务并发更新同一个 agent
        for i in 0..10 {
            let cache_clone = cache.clone();
            let handle = tokio::spawn(async move {
                for j in 0..10 {
                    let timestamp = i * 10 + j;
                    cache_clone
                        .update(create_test_metrics("agent-1", timestamp))
                        .await;
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // 应该有 100 条记录（如果都成功）
        let history = cache.get_history("agent-1", 200).await;
        assert_eq!(history.len(), 100);
    }

    #[tokio::test]
    async fn test_cache_concurrent_reads() {
        let cache = std::sync::Arc::new(Cache::new(100));

        // 先写入数据
        for i in 1..=10 {
            cache.update(create_test_metrics("agent-1", i)).await;
        }

        let mut handles = vec![];

        // 10 个任务并发读取
        for _ in 0..10 {
            let cache_clone = cache.clone();
            let handle = tokio::spawn(async move { cache_clone.get_latest("agent-1").await });
            handles.push(handle);
        }

        // 验证所有读取都成功
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_some());
            assert_eq!(result.unwrap().timestamp, 10);
        }
    }

    #[tokio::test]
    async fn test_cache_get_all_agents_after_removal() {
        let cache = Cache::new(1);

        cache.update(create_test_metrics("agent-1", 1)).await;
        cache.update(create_test_metrics("agent-2", 2)).await;

        // 由于 max_size = 1，每个 agent 只保留 1 条
        let agents = cache.get_all_agents().await;
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_cache_with_large_dataset() {
        let cache = Cache::new(1000);

        // 写入大量数据
        for i in 0..500 {
            cache.update(create_test_metrics("agent-1", i)).await;
        }

        let history = cache.get_history("agent-1", 200).await;
        assert_eq!(history.len(), 200);
        // 应该是最后 200 条
        assert_eq!(history[0].timestamp, 300);
        assert_eq!(history[199].timestamp, 499);
    }

    #[tokio::test]
    async fn test_cache_special_agent_ids() {
        let cache = Cache::new(10);

        let special_ids = vec![
            "agent-with-dash",
            "agent_with_underscore",
            "agent.with.dots",
            "agent@with@at",
            "agent:with:colon",
            "agent/with/slash",
        ];

        for id in &special_ids {
            cache.update(create_test_metrics(id, 1000)).await;
        }

        let agents = cache.get_all_agents().await;
        assert_eq!(agents.len(), 6);

        for id in &special_ids {
            assert!(agents.contains(&id.to_string()));
            assert!(cache.get_latest(id).await.is_some());
        }
    }

    #[tokio::test]
    async fn test_cache_clone() {
        let cache = Cache::new(10);
        let cache_clone = cache.clone();

        cache.update(create_test_metrics("agent-1", 1000)).await;

        // clone 的实例应该能访问相同的数据
        let latest = cache_clone.get_latest("agent-1").await.unwrap();
        assert_eq!(latest.timestamp, 1000);

        // 从 clone 更新
        cache_clone
            .update(create_test_metrics("agent-1", 2000))
            .await;

        // 原实例应该能看到更新
        let latest = cache.get_latest("agent-1").await.unwrap();
        assert_eq!(latest.timestamp, 2000);
    }
}
