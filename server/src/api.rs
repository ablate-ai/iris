use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::storage::Storage;
use common::proto::MetricsRequest;

#[derive(RustEmbed)]
#[folder = "../web"]
struct WebAssets;

#[derive(Clone)]
pub struct ApiState {
    pub storage: Storage,
}

/// Agent 信息响应
#[derive(Serialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub last_seen: i64,
    pub hostname: String,
}

/// 指标历史查询参数
#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

/// API 响应包装
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn error(message: String) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            message: Some(message),
        }
    }
}

/// 创建 HTTP API 路由
pub fn create_router(storage: Storage) -> Router {
    let state = ApiState { storage };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    info!("Web UI 已嵌入二进制，启用静态文件服务");

    Router::new()
        .route("/api", get(root))
        .route("/api/agents", get(list_agents))
        .route("/api/agents/:id/metrics", get(get_agent_metrics))
        .route("/api/agents/:id/metrics/history", get(get_agent_history))
        .fallback(serve_embedded_file)
        .layer(cors)
        .with_state(Arc::new(state))
}

/// 服务嵌入的静态文件
async fn serve_embedded_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // 如果路径为空或是目录，返回 index.html
    let path = if path.is_empty() || path.ends_with('/') {
        "index.html"
    } else {
        path
    };

    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // 对于 SPA，未找到的路径返回 index.html
            if let Some(index) = WebAssets::get("index.html") {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(index.data))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("404 Not Found"))
                    .unwrap()
            }
        }
    }
}

/// 根路径
async fn root() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "Iris API",
        "version": "0.1.0",
        "endpoints": [
            "GET /api/agents",
            "GET /api/agents/:id/metrics",
            "GET /api/agents/:id/metrics/history?limit=100"
        ]
    }))
}

/// 获取所有 Agent 列表
async fn list_agents(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ApiResponse<Vec<AgentInfo>>>, StatusCode> {
    let agent_ids = state.storage.get_all_agents().await;

    let mut agents = Vec::new();
    for agent_id in agent_ids {
        if let Some(latest) = state.storage.get_agent_latest(&agent_id).await {
            agents.push(AgentInfo {
                agent_id: latest.agent_id.clone(),
                last_seen: latest.timestamp,
                hostname: latest.hostname.clone(),
            });
        }
    }

    info!("API: 返回 {} 个 Agent", agents.len());
    Ok(Json(ApiResponse::ok(agents)))
}

/// 获取指定 Agent 的最新指标
async fn get_agent_metrics(
    State(state): State<Arc<ApiState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<ApiResponse<MetricsRequest>>, StatusCode> {
    match state.storage.get_agent_latest(&agent_id).await {
        Some(metrics) => {
            info!("API: 返回 {} 的最新指标", agent_id);
            Ok(Json(ApiResponse::ok(metrics)))
        }
        None => {
            info!("API: Agent {} 不存在", agent_id);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// 获取指定 Agent 的历史指标
async fn get_agent_history(
    State(state): State<Arc<ApiState>>,
    Path(agent_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<ApiResponse<Vec<MetricsRequest>>>, StatusCode> {
    let history = state.storage.get_agent_history(&agent_id, query.limit).await;

    if history.is_empty() {
        info!("API: Agent {} 没有历史数据", agent_id);
        Err(StatusCode::NOT_FOUND)
    } else {
        info!("API: 返回 {} 的 {} 条历史记录", agent_id, history.len());
        Ok(Json(ApiResponse::ok(history)))
    }
}
