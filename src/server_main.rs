use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "iris-server")]
#[command(about = "Iris Server - 监控数据中心服务器", long_about = None)]
struct Cli {
    /// gRPC 监听地址
    #[arg(short, long, default_value = "0.0.0.0:50051")]
    addr: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "iris=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    server::ProbeServer::run(cli.addr).await?;

    Ok(())
}
