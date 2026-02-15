# 部署指南

本文档介绍如何将 Iris 配置为系统服务，实现持久化运行。

## 架构说明

Iris 采用 **Agent 主动上报（Push）** 模式：
- **Agent**: 定期采集系统指标，主动推送到 Server
- **Server**: 接收 Agent 上报的数据，提供 API 和 Web UI

因此，**Agent 必须配置 Server 地址**才能正常工作。

## Linux (systemd)

### 部署 Server

```bash
# 1. 编译并复制二进制文件
cargo build --release --bin iris-server
sudo mkdir -p /opt/iris
sudo cp target/release/iris-server /opt/iris/

# 2. 创建用户和目录
sudo useradd -r -s /bin/false iris
sudo mkdir -p /var/lib/iris
sudo chown iris:iris /var/lib/iris

# 3. 安装 systemd 服务
sudo cp deploy/systemd/iris-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable iris-server
sudo systemctl start iris-server

# 4. 查看状态
sudo systemctl status iris-server
sudo journalctl -u iris-server -f
```

### 部署 Agent

```bash
# 1. 编译并复制二进制文件
cargo build --release --bin iris-agent
sudo mkdir -p /opt/iris
sudo cp target/release/iris-agent /opt/iris/

# 2. 修改配置（重要！）
sudo cp deploy/systemd/iris-agent.service /etc/systemd/system/
sudo nano /etc/systemd/system/iris-agent.service

# 修改以下行中的 server 地址：
# ExecStart=/opt/iris/iris-agent --server http://your-server:50051 --interval 10
# 改为实际的 Server 地址，例如：
# ExecStart=/opt/iris/iris-agent --server http://192.168.1.100:50051 --interval 10

# 3. 启动服务
sudo systemctl daemon-reload
sudo systemctl enable iris-agent
sudo systemctl start iris-agent

# 4. 查看状态
sudo systemctl status iris-agent
sudo journalctl -u iris-agent -f
```

### systemd 常用命令

```bash
# 启动服务
sudo systemctl start iris-server
sudo systemctl start iris-agent

# 停止服务
sudo systemctl stop iris-server
sudo systemctl stop iris-agent

# 重启服务
sudo systemctl restart iris-server
sudo systemctl restart iris-agent

# 查看状态
sudo systemctl status iris-server
sudo systemctl status iris-agent

# 查看日志
sudo journalctl -u iris-server -f
sudo journalctl -u iris-agent -f

# 开机自启
sudo systemctl enable iris-server
sudo systemctl enable iris-agent

# 禁用开机自启
sudo systemctl disable iris-server
sudo systemctl disable iris-agent
```

## macOS (launchd)

### 部署 Server

```bash
# 1. 编译并复制二进制文件
cargo build --release --bin iris-server
sudo cp target/release/iris-server /usr/local/bin/

# 2. 创建工作目录和日志目录
sudo mkdir -p /usr/local/var/iris
sudo mkdir -p /usr/local/var/log

# 3. 安装 launchd 服务
sudo cp deploy/launchd/com.iris.server.plist /Library/LaunchDaemons/
sudo launchctl load /Library/LaunchDaemons/com.iris.server.plist

# 4. 查看状态
sudo launchctl list | grep iris
tail -f /usr/local/var/log/iris-server.log
```

### 部署 Agent

```bash
# 1. 编译并复制二进制文件
cargo build --release --bin iris-agent
sudo cp target/release/iris-agent /usr/local/bin/

# 2. 修改配置（重要！）
# 编辑 deploy/launchd/com.iris.agent.plist
# 找到以下行：
#   <string>http://your-server:50051</string>
# 改为实际的 Server 地址，例如：
#   <string>http://192.168.1.100:50051</string>

nano deploy/launchd/com.iris.agent.plist  # 修改 server 地址

# 3. 安装 launchd 服务
sudo cp deploy/launchd/com.iris.agent.plist /Library/LaunchDaemons/
sudo launchctl load /Library/LaunchDaemons/com.iris.agent.plist

# 4. 查看状态
sudo launchctl list | grep iris
tail -f /usr/local/var/log/iris-agent.log
```

### launchd 常用命令

```bash
# 加载服务
sudo launchctl load /Library/LaunchDaemons/com.iris.server.plist
sudo launchctl load /Library/LaunchDaemons/com.iris.agent.plist

# 卸载服务
sudo launchctl unload /Library/LaunchDaemons/com.iris.server.plist
sudo launchctl unload /Library/LaunchDaemons/com.iris.agent.plist

# 启动服务
sudo launchctl start com.iris.server
sudo launchctl start com.iris.agent

# 停止服务
sudo launchctl stop com.iris.server
sudo launchctl stop com.iris.agent

# 查看服务列表
sudo launchctl list | grep iris

# 查看日志
tail -f /usr/local/var/log/iris-server.log
tail -f /usr/local/var/log/iris-agent.log
```

## 快速方案 (nohup)

如果不想配置系统服务，可以使用 nohup 快速运行：

```bash
# Server
nohup ./target/release/iris-server --addr 0.0.0.0:50051 > server.log 2>&1 &

# Agent
nohup ./target/release/iris-agent --server http://your-server:50051 --interval 10 > agent.log 2>&1 &

# 查看进程
ps aux | grep iris

# 停止进程
pkill iris-server
pkill iris-agent
```

## Docker 部署

如果需要 Docker 部署方案，可以参考以下 Dockerfile：

```dockerfile
# Server
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin iris-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/iris-server /usr/local/bin/
EXPOSE 50051 50052
CMD ["iris-server", "--addr", "0.0.0.0:50051"]
```

```bash
# 构建镜像
docker build -t iris-server -f Dockerfile.server .
docker build -t iris-agent -f Dockerfile.agent .

# 运行容器
docker run -d --name iris-server -p 50051:50051 -p 50052:50052 iris-server
docker run -d --name iris-agent -e SERVER=http://server:50051 iris-agent
```

## 安全建议

1. **防火墙配置**：
   - Server: 开放 50051 (gRPC) 和 50052 (HTTP) 端口
   - Agent: 无需开放端口（仅出站连接）

2. **TLS 加密**：
   - 生产环境建议启用 gRPC TLS 加密
   - 使用反向代理（Nginx/Caddy）为 Web UI 添加 HTTPS

3. **资源限制**：
   - systemd 服务已配置基本的资源限制
   - 可根据实际情况调整 `LimitNOFILE` 等参数

4. **日志管理**：
   - systemd: 使用 `journalctl` 查看日志，自动轮转
   - launchd: 日志文件需要手动配置 logrotate
   - 建议配置日志轮转避免磁盘占满
