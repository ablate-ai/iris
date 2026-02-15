use anyhow::Result;
use common::proto::probe_service_server::{ProbeService, ProbeServiceServer};
use common::proto::{
    HeartbeatRequest, HeartbeatResponse, MetricsRequest, MetricsResponse, StreamResponse,
};
use common::utils::current_timestamp_ms;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tonic::{transport::Server, Request, Response, Status};
use tracing::info;

mod api;
mod storage;

pub struct ProbeServer {
    storage: storage::Storage,
    broadcast: broadcast::Sender<MetricsRequest>,
}

impl ProbeServer {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            storage: storage::Storage::new(),
            broadcast: tx,
        }
    }

    pub async fn run(addr: String) -> Result<()> {
        let grpc_addr: std::net::SocketAddr = addr.parse()?;
        let server = ProbeServer::new();
        let storage = server.storage.clone();
        let broadcast = server.broadcast.clone();

        // 启动 HTTP API 服务器（端口 +1）
        let http_port = grpc_addr.port() + 1;
        let http_addr = format!("{}:{}", grpc_addr.ip(), http_port);
        let http_addr_clone = http_addr.clone();

        tokio::spawn(async move {
            let app = api::create_router(storage, broadcast);
            let listener = tokio::net::TcpListener::bind(&http_addr_clone)
                .await
                .expect("无法绑定 HTTP 端口");

            info!("HTTP API 启动在 http://{}", http_addr_clone);
            axum::serve(listener, app)
                .await
                .expect("HTTP 服务器启动失败");
        });

        info!("gRPC Server 启动在 {}", grpc_addr);

        Server::builder()
            .add_service(ProbeServiceServer::new(server))
            .serve(grpc_addr)
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

        // 广播给前端
        let _ = self.broadcast.send(req.clone());

        // 存储指标数据（可选：降采样）
        if req.timestamp % 10000 == 0 {
            self.storage.save_metrics(&req).await;
        }

        let response = MetricsResponse {
            success: true,
            message: "指标接收成功".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn stream_metrics(
        &self,
        request: Request<tonic::Streaming<MetricsRequest>>,
    ) -> Result<Response<StreamResponse>, Status> {
        let mut stream = request.into_inner();
        let broadcast = self.broadcast.clone();
        let storage = self.storage.clone();

        tokio::spawn(async move {
            let mut agent_id = String::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(metrics) => {
                        if agent_id.is_empty() {
                            agent_id = metrics.agent_id.clone();
                            info!("Agent {} 建立流式连接", agent_id);
                        }

                        // 1. 立即广播给前端（实时）
                        let _ = broadcast.send(metrics.clone());

                        // 2. 降采样存储（每 10 秒存一次）
                        if metrics.timestamp % 10000 == 0 {
                            storage.save_metrics(&metrics).await;
                        }
                    }
                    Err(e) => {
                        info!("Agent {} 流式连接错误: {}", agent_id, e);
                        break;
                    }
                }
            }

            info!("Agent {} 断开流式连接", agent_id);
        });

        Ok(Response::new(StreamResponse {
            success: true,
            message: "流式连接已建立".to_string(),
        }))
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
