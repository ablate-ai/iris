#!/bin/bash
# Iris Server 卸载脚本 (Linux systemd)

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${RED}=== Iris Server 卸载脚本 ===${NC}"
echo -e "${YELLOW}警告: 此操作将停止并删除 Iris Server 服务${NC}\n"

# 检查是否为 root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}错误: 请使用 sudo 运行此脚本${NC}"
    exit 1
fi

# 检查服务是否存在
if ! systemctl list-unit-files | grep -q "iris-server.service"; then
    echo -e "${YELLOW}未找到 iris-server 服务，可能已经卸载${NC}"
    exit 0
fi

# 显示当前状态
echo -e "${BLUE}当前服务状态:${NC}"
systemctl status iris-server --no-pager -l || true
echo ""

# 询问是否删除数据
echo -e "${YELLOW}重要提示: 数据库文件位于 /var/lib/iris/metrics.redb${NC}"
echo -e "${YELLOW}如果删除，所有历史监控数据将永久丢失！${NC}\n"

read -p "是否删除数据库数据? (yes/no): " DELETE_DATA
echo ""

if [ "$DELETE_DATA" = "yes" ]; then
    # 确认删除
    echo -e "${RED}再次确认: 真的要删除所有数据吗? 输入 'DELETE' 确认:${NC}"
    read -p "> " CONFIRM_DELETE

    if [ "$CONFIRM_DELETE" != "DELETE" ]; then
        echo -e "${YELLOW}取消删除数据${NC}"
        DELETE_DATA=""
    else
        echo -e "${RED}✓ 将删除所有数据${NC}"
    fi
else
    DELETE_DATA=""
    echo -e "${GREEN}✓ 将保留数据${NC}"
fi

echo ""
read -p "确认继续卸载? (y/n): " CONFIRM
if [ "$CONFIRM" != "y" ]; then
    echo "已取消卸载"
    exit 0
fi

# 停止服务
echo -e "\n${GREEN}[1/5] 停止服务...${NC}"
systemctl stop iris-server || true
systemctl disable iris-server || true
echo "  服务已停止"

# 删除 systemd 服务文件
echo -e "${GREEN}[2/5] 删除服务文件...${NC}"
rm -f /etc/systemd/system/iris-server.service
systemctl daemon-reload
echo "  服务文件已删除"

# 备份数据（如果保留）
if [ -z "$DELETE_DATA" ] && [ -d "/var/lib/iris" ]; then
    echo -e "${GREEN}[3/5] 备份数据...${NC}"
    BACKUP_DIR="/var/lib/iris.backup.$(date +%Y%m%d_%H%M%S)"
    cp -r /var/lib/iris "$BACKUP_DIR"
    echo "  数据已备份到: $BACKUP_DIR"
fi

# 删除数据（如果用户确认）
if [ -n "$DELETE_DATA" ] && [ -d "/var/lib/iris" ]; then
    echo -e "${GREEN}[3/5] 删除数据...${NC}"
    rm -rf /var/lib/iris
    echo -e "${RED}  数据已删除${NC}"
else
    echo -e "${GREEN}[3/5] 保留数据目录...${NC}"
    echo "  数据目录: /var/lib/iris (保留)"
fi

# 删除二进制文件和 Web UI
echo -e "${GREEN}[4/5] 删除程序文件...${NC}"
rm -rf /opt/iris
echo "  程序文件已删除"

# 删除用户（可选）
echo -e "${GREEN}[5/5] 清理系统用户...${NC}"
read -p "是否删除 iris 系统用户? (y/n): " DELETE_USER
if [ "$DELETE_USER" = "y" ]; then
    userdel iris 2>/dev/null || true
    echo "  用户 iris 已删除"
else
    echo "  保留用户 iris"
fi

# 完成
echo -e "\n${GREEN}✓ 卸载完成！${NC}"

# 显示保留的内容
if [ -z "$DELETE_DATA" ]; then
    echo -e "\n${BLUE}保留的文件:${NC}"
    echo "  数据目录: /var/lib/iris/"
    if [ -n "$BACKUP_DIR" ]; then
        echo "  备份目录: $BACKUP_DIR"
    fi
fi

echo -e "\n${YELLOW}提示: 如需完全清理，手动执行:${NC}"
echo "  sudo rm -rf /var/lib/iris"
echo "  sudo userdel iris"
