# Iris

分布式服务器探针系统，使用 Rust + gRPC 实现。

## 功能特性

- 🚀 提供两个独立二进制：`iris-agent` 与 `iris-server`
- 📊 实时采集系统指标：CPU、内存、磁盘、网络
- 🔄 基于 gRPC 的高效通信
- 🌐 HTTP REST API 查询接口
- 🎨 现代化 Web UI（单文件 HTML，零构建）
- 💾 数据持久化（redb 嵌入式数据库，支持历史数据查询）

## 快速开始

### 一键安装（推荐）

安装脚本会自动：
- ✅ 下载并安装二进制文件到 `/usr/local/bin`
- ✅ 创建 systemd 服务（支持开机自启）
- ✅ **立即启动服务**
- ✅ 验证启动状态

```bash
# 安装 Server（中心服务器）
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | bash

# 安装 Agent（被监控服务器），连接到指定 Server
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 bash

# 安装 Agent 并自定义显示名称
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 IRIS_HOSTNAME=my-server bash
```

💡 **国内用户加速下载**：使用 GitHub 镜像代理
```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | GITHUB_PROXY=https://ghfast.top/ bash
```

**环境变量说明**：
- `IRIS_SERVER`: Agent 连接的 Server 地址（必需）
- `IRIS_HOSTNAME`: 自定义显示名称（可选，默认使用系统 hostname）
- `GITHUB_PROXY`: GitHub 下载加速代理（可选）

**安装完成后**：
- 🌐 Web UI: http://localhost:50052
- 📊 HTTP API: http://localhost:50052/api/agents
- 💾 数据存储: `/var/lib/iris/metrics.redb`（自动持久化）

**管理服务**：
```bash
# 查看状态
sudo systemctl status iris-server

# 查看日志
sudo journalctl -u iris-server -f

# 重启/停止服务
sudo systemctl restart iris-server
sudo systemctl stop iris-server
```

**数据持久化**：

Server 会自动检测 `/var/lib/iris` 目录：
- ✅ 目录存在：数据持久化到 `/var/lib/iris/metrics.redb`
- ⚠️ 目录不存在：仅内存模式（重启后数据丢失）

如果安装时未创建数据目录，可手动创建：
```bash
sudo mkdir -p /var/lib/iris
sudo chown $(whoami) /var/lib/iris
sudo systemctl restart iris-server
```

**卸载**：

脚本会**自动检测**并卸载已安装的组件（server/agent）：
```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/uninstall.sh | bash
```

更多安装选项请查看 [安装文档](docs/INSTALL.md)

---

### 手动编译运行

如果不使用一键安装，也可以手动编译运行：

```bash
# 编译 Server（中心服务器）
cargo build --release --bin iris-server

# 编译 Agent（监控探针）
cargo build --release --bin iris-agent
```

#### 手动运行 Server（中心服务器）

```bash
./target/release/iris-server --addr 0.0.0.0:50051
```

**Server 启动后提供**：
- **gRPC 服务**: 端口 50051（接收 Agent 上报）
- **HTTP API**: 端口 50052（查询监控数据）
- **Web UI**: http://localhost:50052（监控面板）

#### 手动运行 Agent（被监控服务器）

```bash
./target/release/iris-agent --server http://your-server:50051 --interval 1
```

## 命令行参数

### iris-server

```bash
iris-server [OPTIONS]

Options:
  -a, --addr <ADDR>  gRPC 监听地址 [default: 0.0.0.0:50051]
  -h, --help         显示帮助信息

注意：HTTP API 端口为 gRPC 端口 + 1
```

### iris-agent

```bash
iris-agent [OPTIONS]

Options:
  -s, --server <SERVER>      Server 地址 [default: http://127.0.0.1:50051]
  -i, --interval <INTERVAL>  上报间隔（秒） [default: 1]
  -h, --help                 显示帮助信息
```

## 项目结构

```
iris/
├── src/
│   ├── agent_main.rs     # Agent 二进制入口
│   └── server_main.rs    # Server 二进制入口
├── proto/                # gRPC 协议定义
├── agent/                # Agent 模块
│   ├── lib.rs
│   └── collector.rs      # 系统指标采集
├── server/               # Server 模块
│   ├── lib.rs
│   └── storage/          # 数据存储（缓存 + 持久化 + 清理）
└── common/               # 共享代码
    └── lib.rs            # Proto 定义和工具函数
```

## 采集的指标

- **CPU**: 使用率、核心数、每核使用率、负载均衡
- **内存**: 总量、已使用、可用、Swap
- **磁盘**: 挂载点、容量、使用率、读写字节数
- **网络**: 发送/接收字节数、包数、错误数

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
- 💻 系统详情：磁盘、网络信息
- 🔄 自动刷新：每 5 秒更新数据
- 📱 响应式设计：支持移动端访问

详细说明请查看 [web/README.md](web/README.md)

## 持久化运行

将 Iris 配置为系统服务，支持开机自启和自动重启：

- **Linux (systemd)**: 推荐用于生产环境
- **macOS (launchd)**: 适用于 macOS 系统
- **Docker**: 容器化部署
- **nohup**: 快速临时方案

使用一键安装脚本会自动配置 systemd 服务。更多安装选项请查看 [安装文档](docs/INSTALL.md)

## 开发

```bash
# 运行测试
cargo test --workspace

# 检查代码
cargo clippy

# 格式化代码
cargo fmt
```

## 数据存储

Iris 使用 redb 嵌入式数据库进行数据持久化：

- **存储路径**: `/var/lib/iris/metrics.redb`
- **数据保留**: 默认保留最近 7 天数据（约 604,800 条记录/Agent）
- **自动清理**: 每 6 小时自动清理超出限制的旧数据
- **内存缓存**: 每个 Agent 最新 100 条数据缓存在内存中，提供快速查询

**存储模式**：
- **持久化模式**：`/var/lib/iris` 目录存在时启用，数据写入磁盘
- **内存模式**：目录不存在时启用，数据仅保存在内存中（重启丢失）

## TODO

- [x] 添加 HTTP API 用于查询指标
- [x] Web UI 展示
- [x] 持久化存储（redb 嵌入式数据库）
- [ ] 告警功能
- [ ] 多 Agent 管理
