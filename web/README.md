# Iris Web UI

轻量级的服务器监控 Web 界面，使用单文件 HTML + Vue 3 + ECharts 实现。

## 特性

- 📊 实时监控面板
- 📈 历史趋势图表
- 🎨 现代化 UI 设计
- 📱 响应式布局（支持移动端）
- 🚀 零构建，开箱即用
- 🌐 使用 CDN，无需 npm

## 技术栈

- **Vue 3**: 前端框架（通过 CDN）
- **ECharts 5**: 图表库（通过 CDN）
- **原生 CSS**: 样式设计

## 功能

### Dashboard 首页

- 在线 Agent 数量统计
- 平均 CPU/内存使用率
- Agent 卡片展示
  - 实时 CPU/内存使用率
  - 可视化进度条
  - 最后上报时间

### Agent 详情页

- CPU 使用率历史趋势图
- 内存使用率历史趋势图
- 系统信息
  - CPU 核心数
  - 内存总量/已用/可用
- 磁盘使用情况
- Top 进程列表

## 使用方法

### 1. 启动 Server

```bash
cd /path/to/iris
./target/release/iris server --addr 0.0.0.0:50051
```

Server 会自动启动：
- gRPC 服务: `0.0.0.0:50051`
- HTTP API + Web UI: `http://0.0.0.0:50052`

### 2. 访问 Web UI

在浏览器中打开：

```
http://localhost:50052
```

或者如果 Server 在远程服务器上：

```
http://<server-ip>:50052
```

### 3. 启动 Agent

在需要监控的服务器上运行：

```bash
./target/release/iris agent --server http://<server-ip>:50051 --interval 5
```

几秒钟后，Agent 就会出现在 Web UI 中。

## 自动刷新

Web UI 会每 5 秒自动刷新一次数据，无需手动刷新页面。

## 自定义配置

如果需要修改 API 地址，编辑 `web/index.html` 中的 `apiBase` 配置：

```javascript
data() {
    return {
        apiBase: 'http://localhost:50052',  // 修改为你的 API 地址
        // ...
    };
}
```

## 浏览器兼容性

支持所有现代浏览器：
- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## 截图

### Dashboard
- 显示所有在线 Agent
- 实时 CPU/内存使用率
- 统计概览

### Agent 详情
- 历史趋势图表
- 系统详细信息
- 进程列表

## 开发

由于使用单文件 HTML + CDN 方案，无需任何构建工具：

1. 直接编辑 `web/index.html`
2. 刷新浏览器即可看到效果

## 注意事项

1. **CORS**: Server 已启用 CORS，可以从任何域名访问
2. **数据保留**: 当前使用内存存储，每个 Agent 保留最近 1000 条记录
3. **性能**: 单文件方案适合中小规模部署（< 100 个 Agent）

## 未来计划

- [ ] 暗色模式
- [ ] 告警配置界面
- [ ] 自定义时间范围查询
- [ ] 数据导出功能
- [ ] Agent 分组管理

---

**最后更新**: 2026-02-15
