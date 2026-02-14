use anyhow::Result;
use common::proto::probe_service_client::ProbeServiceClient;
use common::proto::MetricsRequest;
use common::utils::{current_timestamp_ms, generate_agent_id};
use std::time::Duration;
use tracing::{error, info};

mod collector;

pub struct Agent {
    agent_id: String,
    hostname: String,
    server_addr: String,
    interval: Duration,
}

impl Agent {
    pub fn new(server_addr: String, interval_secs: u64) -> Self {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());

        Self {
            agent_id: generate_agent_id(),
            hostname,
            server_addr,
            interval: Duration::from_secs(interval_secs),
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Agent {} 启动，连接到 {}", self.agent_id, self.server_addr);

        let mut client = ProbeServiceClient::connect(self.server_addr.clone()).await?;
        info!("成功连接到 Server");

        let mut interval = tokio::time::interval(self.interval);

        loop {
            interval.tick().await;

            // 采集系统指标
            let metrics = collector::collect_metrics();

            // 上报指标
            let request = tonic::Request::new(MetricsRequest {
                agent_id: self.agent_id.clone(),
                timestamp: current_timestamp_ms(),
                system: Some(metrics),
                hostname: self.hostname.clone(),
            });

            match client.report_metrics(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        info!("指标上报成功");
                    } else {
                        error!("指标上报失败: {}", resp.message);
                    }
                }
                Err(e) => {
                    error!("上报指标时出错: {}", e);
                }
            }
        }
    }
}
