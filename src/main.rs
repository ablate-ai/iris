use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "iris")]
#[command(about = "分布式服务器探针系统", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 运行 Agent 模式（部署在被监控服务器）
    Agent {
        /// Server 地址
        #[arg(short, long, default_value = "http://127.0.0.1:50051")]
        server: String,

        /// 上报间隔（秒）
        #[arg(short, long, default_value = "10")]
        interval: u64,
    },
    /// 运行 Server 模式（中心服务器）
    Server {
        /// 监听地址
        #[arg(short, long, default_value = "0.0.0.0:50051")]
        addr: String,
    },
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

    match cli.command {
        Commands::Agent { server, interval } => {
            let agent = agent::Agent::new(server, interval);
            agent.run().await?;
        }
        Commands::Server { addr } => {
            server::ProbeServer::run(addr).await?;
        }
    }

    Ok(())
}
