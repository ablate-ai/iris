#!/bin/bash
# Iris Agent 部署脚本 (Linux systemd)

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}=== Iris Agent 部署脚本 ===${NC}"

# 检查是否为 root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}错误: 请使用 sudo 运行此脚本${NC}"
    exit 1
fi

# 获取 Server 地址
read -p "请输入 Server 地址 (例如: http://192.168.1.100:50051): " SERVER_ADDR

if [ -z "$SERVER_ADDR" ]; then
    echo -e "${RED}错误: Server 地址不能为空${NC}"
    exit 1
fi

# 获取上报间隔
read -p "请输入上报间隔（秒，默认 10）: " INTERVAL
INTERVAL=${INTERVAL:-10}

echo -e "\n${YELLOW}配置信息:${NC}"
echo "  Server 地址: $SERVER_ADDR"
echo "  上报间隔: ${INTERVAL}秒"
read -p "确认部署? (y/n): " CONFIRM

if [ "$CONFIRM" != "y" ]; then
    echo "已取消部署"
    exit 0
fi

# 复制二进制文件
echo -e "\n${GREEN}[1/4] 复制二进制文件...${NC}"
mkdir -p /opt/iris
cp target/release/iris-agent /opt/iris/
chmod +x /opt/iris/iris-agent

# 生成 systemd 服务文件
echo -e "${GREEN}[2/4] 生成 systemd 服务文件...${NC}"
cat > /etc/systemd/system/iris-agent.service <<EOF
[Unit]
Description=Iris Agent - 服务器监控探针
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=/opt/iris
ExecStart=/opt/iris/iris-agent --server $SERVER_ADDR --interval $INTERVAL
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

# 安全加固
NoNewPrivileges=true
PrivateTmp=true

# 资源限制
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

# 重载 systemd
echo -e "${GREEN}[3/4] 重载 systemd...${NC}"
systemctl daemon-reload

# 启动服务
echo -e "${GREEN}[4/4] 启动服务...${NC}"
systemctl enable iris-agent
systemctl start iris-agent

# 检查状态
sleep 2
if systemctl is-active --quiet iris-agent; then
    echo -e "\n${GREEN}✓ 部署成功！${NC}"
    echo -e "\n查看状态: ${YELLOW}sudo systemctl status iris-agent${NC}"
    echo -e "查看日志: ${YELLOW}sudo journalctl -u iris-agent -f${NC}"
else
    echo -e "\n${RED}✗ 服务启动失败，请查看日志:${NC}"
    echo -e "${YELLOW}sudo journalctl -u iris-agent -n 50${NC}"
    exit 1
fi
