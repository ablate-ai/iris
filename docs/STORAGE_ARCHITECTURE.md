# Storage 架构设计

## 概述

Iris Server 采用“内存缓存 + redb 持久化 + 后台清理”三层方案。

## 数据模型

redb 中包含两张表：

1. `metrics`
- Key: `agent_id\0timestamp(20位补零)\0nonce`
- Value: `MetricsRequest` 的 bincode 序列化字节

2. `agent_latest`
- Key: `agent_id`
- Value: 最新时间戳（`i64` 大端字节）

说明：当前实现兼容读取旧 key 格式 `agent_id:timestamp`。

## 存储层结构

1. 内存缓存（`cache.rs`）
- 每个 Agent 默认保留 100 条
- 提供快速读取最新数据/短历史

2. 异步写入队列（`mod.rs`）
- `mpsc` 通道默认容量 1000
- 聚合条件：50 条或 5 秒触发批量落盘

3. 持久化层（`persist.rs`）
- redb 事务写入
- 支持按 Agent 查询历史与最新数据

4. 清理任务（`cleanup.rs`）
- 默认每 6 小时执行
- 默认按数量清理（每 Agent 最大 604,800 条）
- `retention_days` 默认 `0`（不按时间删除）

## 查询策略

1. 最新数据查询：优先内存缓存，缓存未命中再读持久化。
2. 历史查询：
- 仅内存模式：从缓存返回。
- 持久化模式：优先返回持久化历史，避免缓存窗口导致截断。

## 默认配置（`StorageConfig::default`）

```rust
StorageConfig {
    db_path: None,
    cache_size_per_agent: 100,
    batch_size: 50,
    batch_timeout: Duration::from_secs(5),
    channel_capacity: 1000,
    max_records_per_agent: 604_800,
    retention_days: 0,
    cleanup_interval_hours: 6,
    enable_cleanup: true,
}
```

## 模块结构

```text
server/src/storage/
├── mod.rs
├── cache.rs
├── persist.rs
├── cleanup.rs
├── integration_tests.rs
└── performance_tests.rs
```
