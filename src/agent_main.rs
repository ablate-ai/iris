use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "iris-agent")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Iris Agent - 服务器监控探针", long_about = None)]
struct Cli {
    /// Server 地址
    #[arg(short, long, default_value = "http://127.0.0.1:50051")]
    server: String,

    /// 上报间隔（秒）
    #[arg(short, long, default_value = "1")]
    interval: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "iris=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let agent = agent::Agent::new(cli.server, cli.interval);
    agent.run().await?;

    Ok(())
}
