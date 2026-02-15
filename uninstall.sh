#!/bin/bash
set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 默认配置
INSTALL_DIR="/usr/local/bin"

# 打印带颜色的消息
info() {
    echo -e "${BLUE}==>${NC} $1"
}

success() {
    echo -e "${GREEN}✓${NC} $1"
}

error() {
    echo -e "${RED}✗ 错误:${NC} $1" >&2
    exit 1
}

warning() {
    echo -e "${YELLOW}!${NC} $1"
}

# 检测是否有 systemd
has_systemd() {
    command -v systemctl &> /dev/null && systemctl --version &> /dev/null
}

# 检测是否可以使用 sudo（或者当前是 root）
can_sudo_or_root() {
    # 如果是 root 用户
    if [ "$(id -u)" = "0" ]; then
        return 0
    fi

    # 如果有 sudo 命令
    if command -v sudo &> /dev/null; then
        return 0
    fi

    return 1
}

# 获取前缀命令（如果需要就加 sudo）
get_prefix_cmd() {
    if [ "$(id -u)" = "0" ]; then
        echo ""
    else
        echo "sudo"
    fi
}

# 停止并删除 systemd 服务
remove_systemd_service() {
    local service_name=$1

    if ! has_systemd; then
        return 0
    fi

    local service_file="/etc/systemd/system/${service_name}.service"

    # 检查服务文件是否存在
    if [ ! -f "$service_file" ]; then
        info "systemd 服务 ${service_name} 不存在，跳过"
        return 0
    fi

    # 检查是否有权限
    if ! can_sudo_or_root; then
        warning "检测到 systemd 服务 ${service_name} 但没有权限删除（需要 root 或 sudo）"
        warning "请手动删除: ${service_file}"
        return 1
    fi

    local prefix=$(get_prefix_cmd)

    info "停止并禁用服务: ${service_name}"
    ${prefix} systemctl stop "${service_name}" 2>/dev/null || true
    ${prefix} systemctl disable "${service_name}" 2>/dev/null || true

    info "删除服务文件: ${service_file}"
    ${prefix} rm -f "$service_file"

    info "重载 systemd"
    ${prefix} systemctl daemon-reload

    success "已删除服务: ${service_name}"
    return 0
}

# 删除二进制文件
remove_binary() {
    local binary_name=$1
    local binary_path="${INSTALL_DIR}/${binary_name}"

    if [ ! -f "$binary_path" ]; then
        info "二进制文件不存在: ${binary_path}"
        return 0
    fi

    # 检查是否有权限
    if [ -w "$INSTALL_DIR" ] || can_sudo_or_root; then
        local prefix=$(get_prefix_cmd)
        if [ -n "$prefix" ]; then
            ${prefix} rm -f "$binary_path"
        else
            rm -f "$binary_path"
        fi
        success "已删除: ${binary_path}"
    else
        warning "没有权限删除 ${binary_path}，请手动删除"
        return 1
    fi
}

# 检测已安装的组件
detect_installed() {
    local components=()

    # 检查二进制文件
    if [ -f "${INSTALL_DIR}/iris-server" ] || [ -f "${INSTALL_DIR}/iris-server.exe" ]; then
        components+=("server")
    fi

    if [ -f "${INSTALL_DIR}/iris-agent" ] || [ -f "${INSTALL_DIR}/iris-agent.exe" ]; then
        components+=("agent")
    fi

    # 如果没有二进制文件，检查 systemd 服务
    if [ ${#components[@]} -eq 0 ] && has_systemd; then
        if [ -f "/etc/systemd/system/iris-server.service" ]; then
            components+=("server")
        fi
        if [ -f "/etc/systemd/system/iris-agent.service" ]; then
            components+=("agent")
        fi
    fi

    echo "${components[@]}"
}


# 显示使用说明
show_usage() {
    cat << EOF
Iris 卸载脚本

用法:
  bash uninstall.sh          # 自动检测并卸载已安装的组件
  bash uninstall.sh server   # 卸载 server
  bash uninstall.sh agent    # 卸载 agent

环境变量:
  AUTO_CONFIRM              自动确认卸载（默认: false）

示例:
  # 自动检测并卸载（推荐）
  bash uninstall.sh

  # 卸载指定组件
  bash uninstall.sh server
  bash uninstall.sh agent

  # 自动确认卸载（无提示）
  AUTO_CONFIRM=y bash uninstall.sh

EOF
}

# 主函数
main() {
    local mode="$1"
    local components=()

    echo ""
    echo -e "${RED}╔═══════════════════════════════════╗${NC}"
    echo -e "${RED}║   Iris 卸载脚本                   ║${NC}"
    echo -e "${RED}╚═══════════════════════════════════╝${NC}"
    echo ""

    # 如果没有指定模式，自动检测
    if [ -z "$mode" ]; then
        info "自动检测已安装的组件..."
        components=($(detect_installed))

        if [ ${#components[@]} -eq 0 ]; then
            warning "未检测到已安装的 Iris 组件"
            echo "  如果已安装，请手动指定: bash uninstall.sh [server|agent]"
            exit 0
        fi

        info "检测到以下组件: ${components[*]}"
    else
        components=("$mode")
    fi

    # 确认卸载
    if [ -z "${AUTO_CONFIRM:-}" ]; then
        echo -n "确认要卸载 ${components[*]}? [y/N] "
        read -r response
        # 清理输入：去除首尾空格，只取第一个字符
        response=$(echo "$response" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | cut -c1)
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            info "取消卸载"
            exit 0
        fi
    fi

    # 卸载每个组件
    for component in "${components[@]}"; do
        local binary_name="iris-${component}"
        local service_name="iris-${component}"

        echo ""
        info "正在卸载 ${binary_name}..."

        # 1. 停止并删除 systemd 服务
        remove_systemd_service "$service_name"

        # 2. 删除二进制文件
        remove_binary "$binary_name"

        # Windows 平台的 .exe 文件
        if [ -f "${INSTALL_DIR}/${binary_name}.exe" ]; then
            remove_binary "${binary_name}.exe"
        fi
    done

    echo ""
    success "卸载完成！"
    echo ""
}

# 处理参数
case "${1:-}" in
    -h|--help)
        show_usage
        exit 0
        ;;
    server|agent)
        main "$1"
        ;;
    "")
        main ""
        ;;
    *)
        error "未知参数: $1"
        echo ""
        show_usage
        exit 1
        ;;
esac
