use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;

use crate::storage::Storage;
use common::proto::MetricsRequest;

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

    // 检查 web 目录是否存在
    let web_dir = std::path::Path::new("web");
    let serve_web = if web_dir.exists() {
        info!("Web UI 目录存在，启用静态文件服务");
        true
    } else {
        info!("Web UI 目录不存在，仅提供 API 服务");
        false
    };

    let mut router = Router::new()
        .route("/api", get(root))
        .route("/api/agents", get(list_agents))
        .route("/api/agents/:id/metrics", get(get_agent_metrics))
        .route("/api/agents/:id/metrics/history", get(get_agent_history))
        .layer(cors)
        .with_state(Arc::new(state));

    // 如果 web 目录存在，添加静态文件服务
    if serve_web {
        router = router.nest_service("/", ServeDir::new("web"));
    }

    router
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
