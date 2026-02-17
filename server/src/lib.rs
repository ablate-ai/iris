use anyhow::Result;
use common::proto::probe_service_server::{ProbeService, ProbeServiceServer};
use common::proto::{
    HeartbeatRequest, HeartbeatResponse, MetricsRequest, MetricsResponse, StreamResponse,
};
use common::utils::current_timestamp_ms;
use std::path::Path;
use tokio::signal;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tonic::{transport::Server, Request, Response, Status};
use tracing::info;

mod api;
mod assets;
mod storage;

pub struct ProbeServer {
    storage: std::sync::Arc<storage::Storage>,
    broadcast: broadcast::Sender<MetricsRequest>,
}

impl ProbeServer {
    pub fn new() -> Result<Self> {
        // 检查生产环境数据目录是否存在
        let persist_enabled = std::path::Path::new("/var/lib/iris").exists();

        if persist_enabled {
            info!("生产环境模式：数据将持久化到 /var/lib/iris/metrics.redb");
            Self::with_db_path("/var/lib/iris/metrics.redb")
        } else {
            info!("开发环境模式：数据仅保存在内存中（不持久化）");
            Self::memory_only()
        }
    }

    /// 使用指定数据库路径创建 ProbeServer（持久化模式）
    pub fn with_db_path(db_path: &str) -> Result<Self> {
        // 确保 data 目录存在
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let (tx, _) = broadcast::channel(1000);

        // 使用配置创建 Storage（持久化）
        let config = storage::StorageConfig {
            db_path: Some(db_path.to_string()),
            ..Default::default()
        };
        let storage = std::sync::Arc::new(storage::Storage::with_config(config));

        info!("Storage initialized with db_path: {}", db_path);

        Ok(Self {
            storage,
            broadcast: tx,
        })
    }

    /// 创建仅内存模式的 ProbeServer（不持久化）
    pub fn memory_only() -> Result<Self> {
        let (tx, _) = broadcast::channel(1000);

        // 使用配置创建 Storage（仅内存）
        let config = storage::StorageConfig {
            db_path: None,
            ..Default::default()
        };
        let storage = std::sync::Arc::new(storage::Storage::with_config(config));

        info!("Storage initialized in memory-only mode");

        Ok(Self {
            storage,
            broadcast: tx,
        })
    }

    pub async fn run(addr: String) -> Result<()> {
        let grpc_addr: std::net::SocketAddr = addr.parse()?;
        let server = ProbeServer::new()?;
        let storage_for_shutdown = server.storage.clone();
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

        // 设置优雅关闭
        let shutdown_signal = async {
            signal::ctrl_c().await.expect("无法监听 Ctrl+C 信号");
            info!("收到关闭信号，正在优雅关闭...");
        };

        // 同时监听 gRPC 服务和关闭信号
        Server::builder()
            .add_service(ProbeServiceServer::new(server))
            .serve_with_shutdown(grpc_addr, shutdown_signal)
            .await?;

        // 关闭 Storage，确保数据全部写入
        info!("正在关闭 Storage...");
        storage_for_shutdown.shutdown().await?;

        info!("服务器已优雅关闭");
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

        // 存储指标数据（等待持久化完成）
        self.storage
            .save_metrics_sync(&req)
            .await
            .map_err(|e| Status::internal(format!("持久化失败: {}", e)))?;

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

                        // 2. 存储所有指标（storage 内部有清理策略）
                        if let Err(e) = storage.save_metrics_sync(&metrics).await {
                            info!("Agent {} 指标持久化失败: {}", agent_id, e);
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
