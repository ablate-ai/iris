# 故障排查指南

## 数据持久化问题

### 问题：更新版本后历史数据丢失

**症状**：
- 每次重启服务器后，前端只显示重启后的数据
- 历史监控数据消失

**原因**：
服务器以内存模式运行，数据未持久化到磁盘。

**检查方法**：

1. 查看服务日志，确认启动模式：
```bash
sudo journalctl -u iris-server -n 50 | grep "模式"
```

如果看到 `开发环境模式：数据仅保存在内存中（不持久化）`，说明未启用持久化。

2. 检查数据目录是否存在：
```bash
ls -ld /var/lib/iris
```

**解决方案**：

1. 创建数据目录：
```bash
sudo mkdir -p /var/lib/iris
sudo chown $(whoami) /var/lib/iris
```

2. 重启服务：
```bash
sudo systemctl restart iris-server
```

3. 验证持久化已启用：
```bash
sudo journalctl -u iris-server -n 50 | grep "生产环境模式"
```

应该看到：`生产环境模式：数据将持久化到 /var/lib/iris/metrics.redb`

4. 确认数据库文件已创建：
```bash
ls -lh /var/lib/iris/metrics.redb
```

### 问题：数据库文件过大

**症状**：
- `/var/lib/iris/metrics.redb` 文件占用大量磁盘空间

**原因**：
- 监控的 Agent 数量较多
- 数据保留时间过长

**解决方案**：

当前默认配置：
- 每个 Agent 保留最近 7 天数据（约 604,800 条记录）
- 每 6 小时自动清理超出限制的旧数据

如需手动清理，可以：

1. 停止服务：
```bash
sudo systemctl stop iris-server
```

2. 删除数据库文件（会丢失所有历史数据）：
```bash
sudo rm /var/lib/iris/metrics.redb
```

3. 重启服务：
```bash
sudo systemctl start iris-server
```

## 服务启动问题

### 问题：服务启动失败

**检查日志**：
```bash
sudo journalctl -u iris-server -n 100 --no-pager
```

**常见错误**：

1. **端口被占用**：
```
Error: Address already in use
```

解决方案：
- 检查端口占用：`sudo lsof -i :50051`
- 停止占用端口的进程或修改监听端口

2. **权限不足**：
```
Permission denied
```

解决方案：
- 确保数据目录权限正确：`sudo chown $(whoami) /var/lib/iris`
- 或以 root 权限运行服务

3. **数据库损坏**：
```
Failed to open database
```

解决方案：
- 备份并删除数据库文件：
```bash
sudo mv /var/lib/iris/metrics.redb /var/lib/iris/metrics.redb.bak
sudo systemctl restart iris-server
```

## Agent 连接问题

### 问题：Agent 无法连接到 Server

**检查方法**：

1. 查看 Agent 日志：
```bash
sudo journalctl -u iris-agent -n 50
```

2. 测试网络连通性：
```bash
# 测试 gRPC 端口
telnet <server-ip> 50051

# 测试 HTTP API
curl http://<server-ip>:50052/api/agents
```

**常见问题**：

1. **防火墙阻止**：
```bash
# 开放端口（CentOS/RHEL）
sudo firewall-cmd --permanent --add-port=50051/tcp
sudo firewall-cmd --permanent --add-port=50052/tcp
sudo firewall-cmd --reload

# 开放端口（Ubuntu/Debian）
sudo ufw allow 50051/tcp
sudo ufw allow 50052/tcp
```

2. **Server 地址配置错误**：
- 确认 Agent 配置的 Server 地址正确
- 查看 Agent 服务配置：`sudo systemctl cat iris-agent`

## Web UI 问题

### 问题：Web UI 无法访问

**检查方法**：

1. 确认 HTTP API 服务正常：
```bash
curl http://localhost:50052/api/agents
```

2. 检查防火墙设置（如果从远程访问）

3. 确认 Server 监听地址：
```bash
sudo systemctl cat iris-server | grep ExecStart
```

应该包含 `--addr 0.0.0.0:50051`（允许远程访问）

### 问题：Web UI 显示空白或无数据

**可能原因**：

1. **没有 Agent 连接**：
- 检查是否有 Agent 正在运行
- 查看 API 返回：`curl http://localhost:50052/api/agents`

2. **浏览器缓存**：
- 清除浏览器缓存
- 使用隐私模式访问

3. **JavaScript 错误**：
- 打开浏览器开发者工具（F12）
- 查看 Console 标签页的错误信息

## 性能问题

### 问题：Server CPU/内存占用过高

**可能原因**：

1. **Agent 数量过多**
2. **上报频率过高**
3. **数据库写入压力大**

**优化建议**：

1. 调整 Agent 上报间隔（默认 10 秒）：
```bash
# 修改为 30 秒
iris-agent --server http://server:50051 --interval 30
```

2. 监控系统资源：
```bash
# 查看进程资源占用
top -p $(pgrep iris-server)

# 查看数据库文件大小
du -h /var/lib/iris/metrics.redb
```

## 获取帮助

如果以上方法无法解决问题，请：

1. 收集日志信息：
```bash
sudo journalctl -u iris-server -n 200 > iris-server.log
sudo journalctl -u iris-agent -n 200 > iris-agent.log
```

2. 提交 Issue：https://github.com/ablate-ai/iris/issues
   - 附上日志文件
   - 描述问题现象和复现步骤
   - 提供系统环境信息（OS、版本等）
