use common::proto::MetricsRequest;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// 简单的内存存储（后续可替换为数据库）
#[derive(Clone)]
pub struct Storage {
    data: Arc<RwLock<Vec<MetricsRequest>>>,
}

impl Storage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn save_metrics(&self, metrics: &MetricsRequest) {
        let mut data = self.data.write().await;
        data.push(metrics.clone());
        info!("已存储指标数据，当前共 {} 条记录", data.len());

        // 简单的清理策略：保留最近 1000 条
        let len = data.len();
        if len > 1000 {
            data.drain(0..len - 1000);
        }
    }

    pub async fn get_latest(&self, limit: usize) -> Vec<MetricsRequest> {
        let data = self.data.read().await;
        let start = if data.len() > limit {
            data.len() - limit
        } else {
            0
        };
        data[start..].to_vec()
    }
}
