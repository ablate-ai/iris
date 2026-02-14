use anyhow::Result;
use common::proto::probe_service_server::{ProbeService, ProbeServiceServer};
use common::proto::{
    HeartbeatRequest, HeartbeatResponse, MetricsRequest, MetricsResponse,
};
use common::utils::current_timestamp_ms;
use tonic::{transport::Server, Request, Response, Status};
use tracing::info;

mod storage;

pub struct ProbeServer {
    storage: storage::Storage,
}

impl ProbeServer {
    pub fn new() -> Self {
        Self {
            storage: storage::Storage::new(),
        }
    }

    pub async fn run(addr: String) -> Result<()> {
        let addr = addr.parse()?;
        let server = ProbeServer::new();

        info!("Server 启动在 {}", addr);

        Server::builder()
            .add_service(ProbeServiceServer::new(server))
            .serve(addr)
            .await?;

        Ok(())
    }
}

#[tonic::async_trait]
impl ProbeService for ProbeServer {
    async fn report_metrics(
        &self,
        request: Request<MetricsRequest>,
    ) -> Result<Response<MetricsResponse>, Status> {
        let req = request.into_inner();
        info!("收到来自 {} 的指标数据", req.agent_id);

        // 存储指标数据
        self.storage.save_metrics(&req).await;

        let response = MetricsResponse {
            success: true,
            message: "指标接收成功".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        info!("收到来自 {} 的心跳", req.agent_id);

        let response = HeartbeatResponse {
            alive: true,
            server_time: current_timestamp_ms(),
        };

        Ok(Response::new(response))
    }
}
