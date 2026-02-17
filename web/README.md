# Iris Web UI

Iris 的 Web UI 由 `iris-server` 内置提供，前端文件为 `web/index.html`。

## 技术栈

- React 18（UMD CDN）
- Babel Standalone（浏览器端 JSX 转换）
- uPlot（时序图表）
- 原生 CSS

## 启动方式

### 1. 启动 Server

```bash
./target/release/iris-server --addr 0.0.0.0:50051
```

Server 同时提供：

- gRPC: `50051`
- HTTP API + Web UI: `50052`

### 2. 启动 Agent

```bash
./target/release/iris-agent --server http://<server-ip>:50051 --interval 1
```

### 3. 访问页面

```text
http://localhost:50052
```

## 前端数据来源

- `GET /api/agents`
- `GET /api/agents/:id/metrics`
- `GET /api/agents/:id/metrics/history?limit=100`
- `GET /api/stream`（SSE 实时推送）

## 自动刷新与实时性

- 页面使用 SSE 接收实时指标
- 历史图表按需拉取对应 Agent 的历史数据

## 存储说明

Web UI 展示的数据由 Server 存储层提供：

- 内存缓存：每个 Agent 最近 100 条
- 持久化（启用时）：`/var/lib/iris/metrics.redb`
- 清理策略：默认按数量清理（每 Agent 最多约 604,800 条）

## 开发说明

该页面为单文件实现，直接编辑 `web/index.html` 即可。
