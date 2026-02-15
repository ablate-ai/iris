use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

/// 嵌入 Web UI 静态资源
#[derive(RustEmbed)]
#[folder = "../web/"]
struct Assets;

/// 获取 MIME 类型
fn get_mime_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// 处理静态文件请求（带路径）
pub async fn serve_asset(Path(path): Path<String>) -> impl IntoResponse {
    serve_static_file(&path)
}

/// 处理根路径
pub async fn serve_index() -> impl IntoResponse {
    serve_static_file("index.html")
}

/// 统一的静态文件处理逻辑
fn serve_static_file(path: &str) -> Response {
    // 路径规范化：移除前导斜杠，处理根路径
    let normalized_path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };

    // 尝试获取嵌入的文件
    match Assets::get(normalized_path) {
        Some(content) => {
            let mime = get_mime_type(normalized_path);

            Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, "public, max-age=604800") // 7 天缓存
                .body(Body::from(content.data.to_vec()))
                .unwrap()
                .into_response()
        }
        None => {
            // 文件不存在，返回 index.html（SPA 路由回退）
            if let Some(index) = Assets::get("index.html") {
                Response::builder()
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(index.data.to_vec()))
                    .unwrap()
                    .into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }
}
