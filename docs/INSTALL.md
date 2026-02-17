# Iris 安装指南

## 一键安装

### 安装 Server（默认）

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | bash
```

### 安装 Agent（需要指定 Server）

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | \
  IRIS_SERVER=http://192.168.1.100:50051 bash
```

### 安装 Agent 并自定义主机名

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | \
  IRIS_SERVER=http://192.168.1.100:50051 IRIS_HOSTNAME=my-server bash
```

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `IRIS_SERVER` | Agent 模式下连接的 Server 地址；设置后安装 `iris-agent`，未设置则安装 `iris-server` | 空 |
| `IRIS_HOSTNAME` | Agent 自定义显示名称（会注入服务环境变量） | 系统 hostname |
| `VERSION` | 安装版本（GitHub Releases tag） | `latest` |
| `INSTALL_DIR` | 二进制安装目录 | `/usr/local/bin` |
| `GITHUB_PROXY` | GitHub 下载代理前缀（例如 `https://mirror.ghproxy.com/`） | 空 |

## 安装行为

安装脚本会根据环境自动执行：

1. 检测平台并下载对应 release 包
2. 安装二进制到 `INSTALL_DIR`
3. 检测 `systemd`（可用时创建并启动服务）
4. Server 模式下尝试创建 `/var/lib/iris` 以启用持久化

## 服务管理

如果系统启用了 `systemd`，可使用：

```bash
sudo systemctl status iris-server
sudo journalctl -u iris-server -f
sudo systemctl restart iris-server
```

Agent 对应服务名为 `iris-agent`。

## 验证安装

```bash
iris-server --version
iris-agent --version

iris-server --help
iris-agent --help
```

## 手动编译安装

```bash
cargo build --release --bin iris-server
cargo build --release --bin iris-agent
```

编译产物：

- `target/release/iris-server`
- `target/release/iris-agent`

## 卸载

推荐使用卸载脚本（自动检测 server/agent）：

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/uninstall.sh | bash
```

也可以本地执行：

```bash
bash uninstall.sh
# 或指定组件
bash uninstall.sh server
bash uninstall.sh agent
```

## 常见问题

1. 权限不足：

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | \
  INSTALL_DIR=~/.local/bin bash
```

2. 下载慢：设置 `GITHUB_PROXY`。

3. 命令找不到：确认 `INSTALL_DIR` 在 `PATH` 中。
