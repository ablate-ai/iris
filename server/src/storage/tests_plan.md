# Storage 模块测试计划

## 需要添加的依赖 (server/Cargo.toml)

```toml
[dependencies]
# 现有依赖...

# Storage 模块依赖
redb = "2.1"
bincode = "1.3"

[dev-dependencies]
# 测试依赖
tempfile = "3.14"      # 临时文件/目录
criterion = "0.5"      # 性能测试（可选）
proptest = "1.5"       # 属性测试（可选）
```

## 测试目录结构

```
server/src/storage/
├── mod.rs              # 主模块
├── persist.rs          # redb 持久化层
├── cache.rs            # 内存缓存层
└── tests/
    ├── mod.rs          # 测试模块入口
    ├── persist_tests.rs    # persist.rs 单元测试
    ├── cache_tests.rs      # cache.rs 单元测试
    ├── integration_tests.rs # 集成测试
    └── performance_tests.rs # 性能测试
```

## 测试覆盖范围

### 1. persist.rs 单元测试

- **测试初始化**: 创建临时数据库
- **写入测试**: 单条写入、批量写入
- **读取测试**: 按 Key 读取、范围查询
- **更新测试**: 覆盖写入
- **删除测试**: 删除单条、范围删除
- **agent_latest 表测试**: 更新和查询最新时间戳
- **错误处理**: 数据库损坏、写入失败等场景
- **并发测试**: 多线程同时读写

### 2. cache.rs 单元测试

- **LRU 缓存逻辑**: 超过容量时淘汰旧数据
- **写入测试**: 添加/更新数据
- **读取测试**: 命中/未命中
- **清除测试**: 清空缓存、清除指定 agent
- **获取最新**: 获取每个 agent 的最新数据
- **并发测试**: 多线程安全

### 3. 集成测试

- **完整写入流程**: 写入 → 缓存 → 队列 → 持久化
- **完整查询流程**: 查询最新（缓存）→ 查询历史（redb）
- **批量写入**: 验证批量写入正确性
- **数据清理**: 验证旧数据被正确清理
- **服务重启**: 数据持久化验证

### 4. 性能测试

- **写入性能**: 单条 vs 批量写入对比
- **查询性能**: 缓存 vs 数据库查询对比
- **并发性能**: 多线程读写性能
- **内存使用**: 长时间运行内存占用

## 测试工具函数

```rust
// tests/common.rs
pub fn create_temp_db() -> tempfile::TempDir {
    // 创建临时目录用于测试数据库
}

pub fn create_test_metrics(agent_id: &str, count: usize) -> Vec<MetricsRequest> {
    // 创建测试用的指标数据
}

pub fn wait_for_flush(storage: &Storage) -> anyhow::Result<()> {
    // 等待批量写入完成
}
```

## 测试清理

所有测试必须使用临时数据库文件，测试结束后自动清理：
- 使用 `tempfile::TempDir` 创建临时目录
- 利用 Rust 的 Drop trait 自动清理
- 不留下任何测试数据
