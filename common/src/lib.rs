// 自动生成的 protobuf 代码
pub mod proto {
    tonic::include_proto!("probe");
}

pub use proto::*;

// 共享工具函数
pub mod utils {
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 获取当前时间戳（毫秒）
    pub fn current_timestamp_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    /// 生成 Agent ID（基于主机名）
    pub fn generate_agent_id() -> String {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());

        format!("agent-{}", hostname)
    }
}
