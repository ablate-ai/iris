#!/bin/bash
# Iris Server 部署脚本 (Linux systemd)

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}=== Iris Server 部署脚本 ===${NC}"

# 检查是否为 root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}错误: 请使用 sudo 运行此脚本${NC}"
    exit 1
fi

# 获取监听地址
read -p "请输入 gRPC 监听地址 (默认: 0.0.0.0:50051): " ADDR
ADDR=${ADDR:-0.0.0.0:50051}

# 获取 Web 目录
read -p "请输入 Web UI 目录 (默认: /opt/iris/web): " WEB_DIR_INPUT
WEB_DIR=${WEB_DIR_INPUT:-/opt/iris/web}

echo -e "\n${YELLOW}配置信息:${NC}"
echo "  gRPC 地址: $ADDR"
echo "  HTTP API: ${ADDR%:*}:$((${ADDR##*:}+1))"
echo "  Web UI: $WEB_DIR"
read -p "确认部署? (y/n): " CONFIRM

if [ "$CONFIRM" != "y" ]; then
    echo "已取消部署"
    exit 0
fi

# 创建用户
echo -e "\n${GREEN}[1/5] 创建系统用户...${NC}"
if ! id -u iris >/dev/null 2>&1; then
    useradd -r -s /bin/false iris
    echo "  已创建用户 iris"
else
    echo "  用户 iris 已存在"
fi

# 复制二进制文件
echo -e "${GREEN}[2/5] 复制二进制文件...${NC}"
mkdir -p /opt/iris
cp target/release/iris-server /opt/iris/
chmod +x /opt/iris/iris-server

# 复制 Web UI 文件
echo -e "${GREEN}[2.5/5] 复制 Web UI 文件...${NC}"
if [ -d "web" ]; then
    cp -r web /opt/iris/
    echo "  Web UI 已复制到 $WEB_DIR"
else
    echo "  警告: web 目录不存在，跳过"
fi

# 创建数据目录
echo -e "${GREEN}[3/5] 创建数据目录...${NC}"
mkdir -p /var/lib/iris
chown iris:iris /var/lib/iris

# 生成 systemd 服务文件
echo -e "${GREEN}[4/5] 生成 systemd 服务文件...${NC}"
cat > /etc/systemd/system/iris-server.service <<EOF
[Unit]
Description=Iris Server - 监控数据中心服务器
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=iris
Group=iris
WorkingDirectory=/opt/iris
ExecStart=/opt/iris/iris-server --addr $ADDR --web-dir $WEB_DIR
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

# 安全加固
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/iris /opt/iris/web

# 资源限制
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

# 重载并启动服务
echo -e "${GREEN}[5/5] 启动服务...${NC}"
systemctl daemon-reload
systemctl enable iris-server
systemctl start iris-server

# 检查状态
sleep 2
if systemctl is-active --quiet iris-server; then
    HTTP_PORT=$((${ADDR##*:}+1))
    echo -e "\n${GREEN}✓ 部署成功！${NC}"
    echo -e "\n访问地址:"
    echo -e "  Web UI:  ${YELLOW}http://$(hostname -I | awk '{print $1}'):${HTTP_PORT}${NC}"
    echo -e "  API:     ${YELLOW}http://$(hostname -I | awk '{print $1}'):${HTTP_PORT}/api${NC}"
    echo -e "\n管理命令:"
    echo -e "  查看状态: ${YELLOW}sudo systemctl status iris-server${NC}"
    echo -e "  查看日志: ${YELLOW}sudo journalctl -u iris-server -f${NC}"
else
    echo -e "\n${RED}✗ 服务启动失败，请查看日志:${NC}"
    echo -e "${YELLOW}sudo journalctl -u iris-server -n 50${NC}"
    exit 1
fi
