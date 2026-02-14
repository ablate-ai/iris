#!/bin/bash

cd /Users/c.chen/dev/iris

echo "=== 启动 Server ==="
RUST_LOG=info ./target/release/iris server > /tmp/iris-server.log 2>&1 &
SERVER_PID=$!
echo "Server PID: $SERVER_PID"

sleep 2

echo ""
echo "=== 启动 Agent ==="
RUST_LOG=info ./target/release/iris agent --interval 3 > /tmp/iris-agent.log 2>&1 &
AGENT_PID=$!
echo "Agent PID: $AGENT_PID"

echo ""
echo "=== 等待 10 秒收集数据 ==="
for i in {10..1}; do
    echo -n "$i... "
    sleep 1
done
echo ""

echo ""
echo "=== Server 日志 ==="
tail -20 /tmp/iris-server.log

echo ""
echo "=== Agent 日志 ==="
tail -20 /tmp/iris-agent.log

echo ""
echo "=== 停止服务 ==="
kill $AGENT_PID 2>/dev/null
kill $SERVER_PID 2>/dev/null

echo ""
echo "✅ 测试完成！如果看到 '指标上报成功' 和 '已存储指标数据'，说明运行正常"
