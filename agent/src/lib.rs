use anyhow::Result;
use common::proto::probe_service_client::ProbeServiceClient;
use common::proto::MetricsRequest;
use common::utils::{current_timestamp_ms, generate_agent_id};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
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
        // 优先使用环境变量 IRIS_HOSTNAME，否则使用系统 hostname
        let hostname = std::env::var("IRIS_HOSTNAME")
            .ok()
            .or_else(|| hostname::get().ok().and_then(|h| h.into_string().ok()))
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

        loop {
            match self.run_stream().await {
                Ok(_) => {
                    info!("流式连接正常结束");
                }
                Err(e) => {
                    collector::increment_errors();
                    error!("流式连接错误: {}，3秒后重连", e);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    async fn run_stream(&self) -> Result<()> {
        let mut client = ProbeServiceClient::connect(self.server_addr.clone()).await?;
        info!("成功连接到 Server，建立流式通道");

        let (tx, rx) = mpsc::channel(100);
        let stream = ReceiverStream::new(rx);

        // 发起流式请求
        let response = client.stream_metrics(stream).await?;
        info!("流式连接已建立: {}", response.into_inner().message);

        let mut interval = tokio::time::interval(self.interval);

        loop {
            interval.tick().await;

            // 采集系统指标
            let metrics = collector::collect_metrics();

            // 通过流发送
            let request = MetricsRequest {
                agent_id: self.agent_id.clone(),
                timestamp: current_timestamp_ms(),
                system: Some(metrics),
                hostname: self.hostname.clone(),
            };

            if tx.send(request).await.is_err() {
                return Err(anyhow::anyhow!("发送指标失败，流已关闭"));
            }

            collector::increment_metrics_sent();
            info!("指标已发送");
        }
    }
}
