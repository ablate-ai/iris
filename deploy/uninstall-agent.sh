#!/bin/bash
# Iris Agent 卸载脚本 (Linux systemd)

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${RED}=== Iris Agent 卸载脚本 ===${NC}"
echo -e "${YELLOW}警告: 此操作将停止并删除 Iris Agent 服务${NC}\n"

# 检查是否为 root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}错误: 请使用 sudo 运行此脚本${NC}"
    exit 1
fi

# 检查服务是否存在
if ! systemctl list-unit-files | grep -q "iris-agent.service"; then
    echo -e "${YELLOW}未找到 iris-agent 服务，可能已经卸载${NC}"
    exit 0
fi

# 显示当前状态
echo -e "${YELLOW}当前服务状态:${NC}"
systemctl status iris-agent --no-pager -l || true
echo ""

# 确认卸载
read -p "确认继续卸载? (y/n): " CONFIRM
if [ "$CONFIRM" != "y" ]; then
    echo "已取消卸载"
    exit 0
fi

# 停止服务
echo -e "\n${GREEN}[1/3] 停止服务...${NC}"
systemctl stop iris-agent || true
systemctl disable iris-agent || true
echo "  服务已停止"

# 删除 systemd 服务文件
echo -e "${GREEN}[2/3] 删除服务文件...${NC}"
rm -f /etc/systemd/system/iris-agent.service
systemctl daemon-reload
echo "  服务文件已删除"

# 删除二进制文件
echo -e "${GREEN}[3/3] 删除程序文件...${NC}"
rm -f /opt/iris/iris-agent
echo "  程序文件已删除"

# 完成
echo -e "\n${GREEN}✓ 卸载完成！${NC}"
echo -e "\n${YELLOW}提示: Agent 不存储数据，无需清理${NC}"
