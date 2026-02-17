# Iris HTTP API 文档

## 概述

Iris 提供 RESTful HTTP API 用于查询监控数据。

- **Base URL**: `http://<server-host>:<http-port>`
- **默认端口**: gRPC 端口 + 1（例如 gRPC 在 50051，HTTP 在 50052）
- **响应格式**: JSON
- **CORS**: 已启用，支持跨域请求

## 通用响应格式

除 `GET /api`（信息端点）与 `GET /api/stream`（SSE）外，业务 API 响应使用以下格式：

```json
{
  "success": true,
  "data": <响应数据>,
  "message": null
}
```

典型错误响应：

```json
{
  "success": false,
  "data": null,
  "message": "错误信息"
}
```

说明：当前实现中部分错误场景会直接返回 HTTP 状态码（如 `404`），不保证返回 JSON body。

## API 端点

### 1. 获取 API 信息

获取 API 基本信息和可用端点列表。

**请求**

```
GET /api
```

**响应示例**

```json
{
  "name": "Iris API",
  "version": "0.1.0",
  "endpoints": [
    "GET /api/stream (SSE)",
    "GET /api/agents",
    "GET /api/agents/:id/metrics",
    "GET /api/agents/:id/metrics/history?limit=100"
  ]
}
```

---

### 2. SSE 实时流

通过 Server-Sent Events (SSE) 推送实时指标。

**请求**

```
GET /api/stream
```

**响应说明**

- `Content-Type`: `text/event-stream`
- 每条事件的 `data` 为一条 `MetricsRequest` JSON
- 服务端会定期发送 keep-alive 注释，避免连接被中间层关闭

---

### 3. 获取所有 Agent 列表

获取所有已连接的 Agent 信息。

**请求**

```
GET /api/agents
```

**响应示例**

```json
{
  "success": true,
  "data": [
    {
      "agent_id": "agent-server01",
      "last_seen": 1771093719588,
      "hostname": "server01"
    },
    {
      "agent_id": "agent-server02",
      "last_seen": 1771093720123,
      "hostname": "server02"
    }
  ],
  "message": null
}
```

**字段说明**

- `agent_id`: Agent 唯一标识
- `last_seen`: 最后上报时间（Unix 时间戳，毫秒）
- `hostname`: 主机名

---

### 4. 获取指定 Agent 的最新指标

获取指定 Agent 的最新一次上报的完整指标数据。

**请求**

```
GET /api/agents/:id/metrics
```

**路径参数**

- `id`: Agent ID（例如 `agent-server01`）

**响应示例**

```json
{
  "success": true,
  "data": {
    "agent_id": "agent-server01",
    "hostname": "server01",
    "timestamp": 1771093729583,
    "system": {
      "cpu": {
        "usage_percent": 21.87,
        "core_count": 8,
        "per_core": [15.2, 18.5, 22.1, 25.3, 20.0, 19.8, 23.4, 21.0],
        "load_avg_1": 2.5,
        "load_avg_5": 2.1,
        "load_avg_15": 1.8
      },
      "memory": {
        "total": 17179869184,
        "used": 11872411648,
        "available": 5307457536,
        "usage_percent": 69.13,
        "swap_total": 4294967296,
        "swap_used": 1073741824
      },
      "disks": [
        {
          "mount_point": "/",
          "device": "/dev/sda1",
          "total": 1000000000000,
          "used": 500000000000,
          "available": 500000000000,
          "usage_percent": 50.0,
          "read_bytes": 1234567890,
          "write_bytes": 9876543210
        }
      ],
      "network": {
        "bytes_sent": 123456789,
        "bytes_recv": 987654321,
        "packets_sent": 100000,
        "packets_recv": 150000,
        "errors_in": 0,
        "errors_out": 0
      }
    }
  },
  "message": null
}
```

**错误响应**

- `404 Not Found`: Agent 不存在

---

### 5. 获取指定 Agent 的历史指标

获取指定 Agent 的历史指标数据。

**请求**

```
GET /api/agents/:id/metrics/history?limit=100
```

**路径参数**

- `id`: Agent ID（例如 `agent-server01`）

**查询参数**

- `limit`: 返回的记录数量（默认 100）

**响应示例**

```json
{
  "success": true,
  "data": [
    {
      "agent_id": "agent-server01",
      "hostname": "server01",
      "timestamp": 1771093719588,
      "system": { ... }
    },
    {
      "agent_id": "agent-server01",
      "hostname": "server01",
      "timestamp": 1771093729583,
      "system": { ... }
    }
  ],
  "message": null
}
```

**说明**

- 返回的数据按时间戳升序排列
- 数据结构与"获取最新指标"相同

**错误响应**

- `404 Not Found`: Agent 不存在或没有历史数据

---

## 使用示例

### cURL

```bash
# 获取所有 Agent
curl http://localhost:50052/api/agents

# SSE 实时订阅
curl -N http://localhost:50052/api/stream

# 获取指定 Agent 的最新指标
curl http://localhost:50052/api/agents/agent-server01/metrics

# 获取历史数据（最近 50 条）
curl "http://localhost:50052/api/agents/agent-server01/metrics/history?limit=50"
```

### JavaScript (Fetch API)

```javascript
// 获取所有 Agent
fetch('http://localhost:50052/api/agents')
  .then(res => res.json())
  .then(data => console.log(data.data));

// 获取最新指标
fetch('http://localhost:50052/api/agents/agent-server01/metrics')
  .then(res => res.json())
  .then(data => {
    const metrics = data.data;
    console.log(`CPU: ${metrics.system.cpu.usage_percent}%`);
    console.log(`Memory: ${metrics.system.memory.usage_percent}%`);
  });
```

### Python (requests)

```python
import requests

# 获取所有 Agent
response = requests.get('http://localhost:50052/api/agents')
agents = response.json()['data']

for agent in agents:
    print(f"Agent: {agent['agent_id']}, Hostname: {agent['hostname']}")

# 获取最新指标
response = requests.get('http://localhost:50052/api/agents/agent-server01/metrics')
metrics = response.json()['data']
print(f"CPU: {metrics['system']['cpu']['usage_percent']}%")
```

---

## 数据类型说明

### CPU 指标 (CpuMetrics)

| 字段 | 类型 | 说明 |
|------|------|------|
| usage_percent | float | CPU 总使用率（%） |
| core_count | int | CPU 核心数 |
| per_core | float[] | 每个核心的使用率（%） |
| load_avg_1 | float | 1 分钟平均负载 |
| load_avg_5 | float | 5 分钟平均负载 |
| load_avg_15 | float | 15 分钟平均负载 |

### 内存指标 (MemoryMetrics)

| 字段 | 类型 | 说明 |
|------|------|------|
| total | uint64 | 总内存（字节） |
| used | uint64 | 已使用内存（字节） |
| available | uint64 | 可用内存（字节） |
| usage_percent | float | 内存使用率（%） |
| swap_total | uint64 | Swap 总量（字节） |
| swap_used | uint64 | Swap 已使用（字节） |

### 磁盘指标 (DiskMetrics)

| 字段 | 类型 | 说明 |
|------|------|------|
| mount_point | string | 挂载点 |
| device | string | 设备名 |
| total | uint64 | 总容量（字节） |
| used | uint64 | 已使用（字节） |
| available | uint64 | 可用（字节） |
| usage_percent | float | 使用率（%） |
| read_bytes | uint64 | 累计读取字节数 |
| write_bytes | uint64 | 累计写入字节数 |

### 网络指标 (NetworkMetrics)

| 字段 | 类型 | 说明 |
|------|------|------|
| bytes_sent | uint64 | 累计发送字节数 |
| bytes_recv | uint64 | 累计接收字节数 |
| packets_sent | uint64 | 累计发送包数 |
| packets_recv | uint64 | 累计接收包数 |
| errors_in | uint64 | 接收错误数 |
| errors_out | uint64 | 发送错误数 |

## 错误码

| HTTP 状态码 | 说明 |
|------------|------|
| 200 | 请求成功 |
| 404 | 资源不存在（Agent 不存在或无数据） |
| 500 | 服务器内部错误 |

---

## 注意事项

1. **数据保留**:
   - 内存缓存每个 Agent 默认 100 条
   - 持久化启用时默认按数量清理，每个 Agent 最多约 604,800 条
2. **时间戳**: 所有时间戳均为 Unix 时间戳（毫秒）
3. **单位**:
   - 内存/磁盘容量单位为字节（Byte）
   - 百分比单位为 0-100
   - 网络流量单位为字节（Byte）
4. **CORS**: API 已启用 CORS，可直接从浏览器跨域访问

---

**最后更新**: 2026-02-15
