# Iris

分布式服务器探针系统，使用 Rust + gRPC 实现。

## 功能特性

- 🚀 单一二进制文件，支持 Agent 和 Server 两种运行模式
- 📊 实时采集系统指标：CPU、内存、磁盘、网络、进程
- 🔄 基于 gRPC 的高效通信
- 🌐 HTTP REST API 查询接口
- 🎨 现代化 Web UI（单文件 HTML，零构建）
- 💾 内存存储（可扩展为数据库）

## 快速开始

### 一键安装（推荐）

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | bash
```

更多安装选项请查看 [安装文档](docs/INSTALL.md)

### 编译

项目提供两种编译方式：

**方式一：编译独立二进制（推荐）**

```bash
# 编译 Server（中心服务器）
cargo build --release --bin iris-server

# 编译 Agent（监控探针）
cargo build --release --bin iris-agent
```

**方式二：编译统一二进制**

```bash
# 编译包含 Agent 和 Server 的统一二进制
cargo build --release --bin iris
```

### 运行 Server（中心服务器）

```bash
# 使用独立二进制
./target/release/iris-server --addr 0.0.0.0:50051

# 或使用统一二进制
./target/release/iris server --addr 0.0.0.0:50051
```

Server 会同时启动：
- **gRPC 服务**: 端口 50051（接收 Agent 上报）
- **HTTP API**: 端口 50052（查询监控数据）
- **Web UI**: http://localhost:50052（监控面板）

### 运行 Agent（被监控服务器）

```bash
# 使用独立二进制
./target/release/iris-agent --server http://your-server:50051 --interval 10

# 或使用统一二进制
./target/release/iris agent --server http://your-server:50051 --interval 10
```

## 命令行参数

### iris-server（独立二进制）

```bash
iris-server [OPTIONS]

Options:
  -a, --addr <ADDR>  gRPC 监听地址 [default: 0.0.0.0:50051]
  -h, --help         显示帮助信息

注意：HTTP API 端口为 gRPC 端口 + 1
```

### iris-agent（独立二进制）

```bash
iris-agent [OPTIONS]

Options:
  -s, --server <SERVER>      Server 地址 [default: http://127.0.0.1:50051]
  -i, --interval <INTERVAL>  上报间隔（秒） [default: 10]
  -h, --help                 显示帮助信息
```

## 项目结构

```
iris/
├── src/main.rs           # 主入口
├── proto/                # gRPC 协议定义
├── agent/                # Agent 模块
│   ├── lib.rs
│   └── collector.rs      # 系统指标采集
├── server/               # Server 模块
│   ├── lib.rs
│   └── storage.rs        # 数据存储
└── common/               # 共享代码
    └── lib.rs            # Proto 定义和工具函数
```

## 采集的指标

- **CPU**: 使用率、核心数、每核使用率、负载均衡
- **内存**: 总量、已使用、可用、Swap
- **磁盘**: 挂载点、容量、使用率、读写字节数
- **网络**: 发送/接收字节数、包数、错误数
- **进程**: Top 10 进程的 CPU、内存使用情况

## HTTP API

Server 提供 RESTful API 用于查询监控数据：

```bash
# 获取所有 Agent 列表
curl http://localhost:50052/api/agents

# 获取指定 Agent 的最新指标
curl http://localhost:50052/api/agents/agent-hostname/metrics

# 获取历史数据
curl "http://localhost:50052/api/agents/agent-hostname/metrics/history?limit=100"
```

详细 API 文档请查看 [docs/API.md](docs/API.md)

## Web UI

访问 `http://localhost:50052` 即可打开监控面板。

**功能特性**：
- 📊 Dashboard 首页：所有 Agent 概览、实时指标
- 📈 历史趋势图表：CPU、内存使用率
- 💻 系统详情：磁盘、网络、进程信息
- 🔄 自动刷新：每 5 秒更新数据
- 📱 响应式设计：支持移动端访问

详细说明请查看 [web/README.md](web/README.md)

## 持久化运行

将 Iris 配置为系统服务，支持开机自启和自动重启：

- **Linux (systemd)**: 推荐用于生产环境
- **macOS (launchd)**: 适用于 macOS 系统
- **Docker**: 容器化部署
- **nohup**: 快速临时方案

详细部署指南请查看 [deploy/DEPLOY.md](deploy/DEPLOY.md)

## 开发

```bash
# 运行测试
cargo test

# 检查代码
cargo clippy

# 格式化代码
cargo fmt
```

## TODO

- [x] 添加 HTTP API 用于查询指标
- [x] Web UI 展示
- [ ] 持久化存储（PostgreSQL/InfluxDB）
- [ ] 告警功能
- [ ] 多 Agent 管理
