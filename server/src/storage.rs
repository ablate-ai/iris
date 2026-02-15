use common::proto::MetricsRequest;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// 简单的内存存储（后续可替换为数据库）
#[derive(Clone)]
pub struct Storage {
    // agent_id -> Vec<MetricsRequest>
    data: Arc<RwLock<HashMap<String, Vec<MetricsRequest>>>>,
}

impl Storage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn save_metrics(&self, metrics: &MetricsRequest) {
        let mut data = self.data.write().await;
        let agent_data = data.entry(metrics.agent_id.clone()).or_insert_with(Vec::new);
        agent_data.push(metrics.clone());

        info!("已存储 {} 的指标数据，该 Agent 共 {} 条记录", metrics.agent_id, agent_data.len());

        // 简单的清理策略：每个 Agent 保留最近 1000 条
        if agent_data.len() > 1000 {
            let len = agent_data.len();
            agent_data.drain(0..len - 1000);
        }
    }

    #[allow(dead_code)]
    pub async fn get_latest(&self, limit: usize) -> Vec<MetricsRequest> {
        let data = self.data.read().await;
        let mut all_metrics = Vec::new();

        for metrics in data.values() {
            all_metrics.extend(metrics.iter().cloned());
        }

        // 按时间戳排序
        all_metrics.sort_by_key(|m| m.timestamp);

        let start = if all_metrics.len() > limit {
            all_metrics.len() - limit
        } else {
            0
        };
        all_metrics[start..].to_vec()
    }

    pub async fn get_all_agents(&self) -> Vec<String> {
        let data = self.data.read().await;
        data.keys().cloned().collect()
    }

    pub async fn get_agent_latest(&self, agent_id: &str) -> Option<MetricsRequest> {
        let data = self.data.read().await;
        data.get(agent_id).and_then(|metrics| metrics.last().cloned())
    }

    pub async fn get_agent_history(&self, agent_id: &str, limit: usize) -> Vec<MetricsRequest> {
        let data = self.data.read().await;
        if let Some(metrics) = data.get(agent_id) {
            let start = if metrics.len() > limit {
                metrics.len() - limit
            } else {
                0
            };
            metrics[start..].to_vec()
        } else {
            Vec::new()
        }
    }
}
