#!/bin/bash
set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 默认配置
REPO="ablate-ai/iris"
INSTALL_DIR="/usr/local/bin"

# 配置了 IRIS_SERVER 就是 agent，否则是 server
IRIS_SERVER="${IRIS_SERVER:-}"
IRIS_HOSTNAME="${IRIS_HOSTNAME:-}"

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

# 创建并启动 systemd 服务
setup_systemd_service() {
    local service_name=$1
    local binary_name=$2
    local exec_args=$3
    local env_vars=$4
    local working_dir=$5

    if ! has_systemd; then
        return 1
    fi

    # 检查是否有权限创建服务
    if ! can_sudo_or_root; then
        warning "检测到 systemd 但没有权限创建服务（需要 root 或 sudo）"
        return 1
    fi

    info "检测到 systemd，创建服务: ${service_name}"

    local prefix=$(get_prefix_cmd)

    # 创建 systemd service 文件
    if [ -n "$prefix" ]; then
        warning "需要 ${prefix} 权限创建 systemd 服务"
    fi

    # 构建 WorkingDirectory 行（如果提供）
    local working_dir_line=""
    if [ -n "$working_dir" ]; then
        working_dir_line="WorkingDirectory=${working_dir}"
    fi

    ${prefix} tee "/etc/systemd/system/${service_name}.service" > /dev/null <<EOF
[Unit]
Description=Iris ${binary_name}
After=network.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/${binary_name} ${exec_args}
${working_dir_line}
${env_vars}
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

    # 重载并启动服务
    info "重载 systemd 并启动 ${service_name}..."
    ${prefix} systemctl daemon-reload
    ${prefix} systemctl enable "${service_name}"
    ${prefix} systemctl restart "${service_name}"

    # 等待启动
    sleep 2

    if ${prefix} systemctl is-active --quiet "${service_name}"; then
        success "${service_name} 已启动"
        return 0
    else
        error "${service_name} 启动失败，请查看日志: ${prefix} journalctl -u ${service_name} -n 50"
    fi
}

# 检测操作系统和架构
detect_platform() {
    local os arch

    # 检测操作系统
    case "$(uname -s)" in
        Linux*)     os="linux" ;;
        Darwin*)    os="darwin" ;;
        MINGW*|MSYS*|CYGWIN*) os="windows" ;;
        *)          error "不支持的操作系统: $(uname -s)" ;;
    esac

    # 检测架构
    case "$(uname -m)" in
        x86_64|amd64)   arch="amd64" ;;
        aarch64|arm64)  arch="arm64" ;;
        *)              error "不支持的架构: $(uname -m)" ;;
    esac

    echo "${os}-${arch}"
}

# 显示使用说明
show_usage() {
    cat << EOF
Iris 一键安装脚本

用法:
  # 安装 server
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash

  # 安装 agent
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 bash

  # 安装 agent 并自定义显示名称
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 IRIS_HOSTNAME=my-server bash

环境变量:
  IRIS_SERVER     server 地址（设置此值则安装 agent，否则安装 server）
  IRIS_HOSTNAME   自定义显示名称（可选，默认使用系统 hostname）
  GITHUB_PROXY    GitHub 代理（用于加速下载，如：https://mirror.ghproxy.com/）

说明:
  Web UI 已嵌入二进制文件，无需额外安装
  自动从 GitHub Releases 下载最新版本

示例:
  # 安装最新版 server
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash

  # 使用代理加速下载
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | GITHUB_PROXY=https://mirror.ghproxy.com/ bash

  # 安装 agent
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 bash

EOF
}

# 下载并安装
install_binary() {
    local binary_name=$1
    local platform=$2

    info "安装 ${binary_name}..."

    # 构建下载 URL（使用 latest 自动重定向到最新版本）
    local ext=""
    if [[ "$platform" == "windows-"* ]]; then
        ext=".exe"
        local archive_name="iris-${platform}.zip"
    else
        local archive_name="iris-${platform}.tar.gz"
    fi

    local download_url="${GITHUB_PROXY}https://github.com/${REPO}/releases/latest/download/${archive_name}"
    local tmp_dir=$(mktemp -d)

    info "下载 ${archive_name}..."
    if ! curl -fsSL -H "User-Agent: iris-installer" "$download_url" -o "${tmp_dir}/${archive_name}"; then
        error "下载失败，请检查网络连接或 GitHub Releases 是否存在文件: ${download_url}"
    fi

    # 解压
    info "解压文件..."
    cd "$tmp_dir"
    if [[ "$platform" == "windows-"* ]]; then
        unzip -q "${archive_name}"
    else
        tar -xzf "${archive_name}"
    fi

    # 安装二进制文件
    local binary_file="${binary_name}-${platform}${ext}"
    if [ ! -f "$binary_file" ]; then
        error "找不到二进制文件: ${binary_file}"
    fi

    # 尝试安装，权限不足则使用 sudo
    if ! install -m 755 "$binary_file" "${INSTALL_DIR}/${binary_name}${ext}" 2>/dev/null; then
        warning "需要 sudo 权限安装到 ${INSTALL_DIR}"
        sudo install -m 755 "$binary_file" "${INSTALL_DIR}/${binary_name}${ext}"
    fi

    # 清理临时文件
    cd - > /dev/null
    rm -rf "$tmp_dir"

    success "${binary_name} 已安装到 ${INSTALL_DIR}/${binary_name}${ext}"
}

# 显示使用说明
show_usage() {
    cat << EOF
Iris 一键安装脚本

用法:
  # 安装 server
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash

  # 安装 agent
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 bash

  # 安装 agent 并自定义显示名称
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 IRIS_HOSTNAME=my-server bash

环境变量:
  IRIS_SERVER     server 地址（设置此值则安装 agent，否则安装 server）
  IRIS_HOSTNAME   自定义显示名称（可选，默认使用系统 hostname）
  VERSION         指定版本号（默认: latest）

说明:
  Web UI 已嵌入二进制文件，无需额外安装

示例:
  # 安装最新版 server
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash

  # 安装指定版本
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | VERSION=v0.1.0 bash

  # 安装 agent
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 bash

EOF
}

# 主函数
main() {
    echo ""
    echo -e "${GREEN}╔═══════════════════════════════════╗${NC}"
    echo -e "${GREEN}║   Iris 安装脚本                   ║${NC}"
    echo -e "${GREEN}╚═══════════════════════════════════╝${NC}"
    echo ""

    # 检查依赖
    for cmd in curl tar; do
        if ! command -v $cmd &> /dev/null; then
            error "需要安装 $cmd"
        fi
    done

    # 检测平台
    local platform
    platform=$(detect_platform)
    info "检测到平台: ${platform}"
    info "从 GitHub Releases 下载最新版本"

    # 创建安装目录
    if [ ! -d "$INSTALL_DIR" ]; then
        warning "创建安装目录: ${INSTALL_DIR}"
        mkdir -p "$INSTALL_DIR" || sudo mkdir -p "$INSTALL_DIR"
    fi

    # 判断安装模式
    if [ -n "$IRIS_SERVER" ]; then
        # agent 模式
        info "检测到 IRIS_SERVER，安装 agent 模式"
        install_binary "iris-agent" "$platform"

        # 准备环境变量
        local env_vars=""
        if [ -n "$IRIS_HOSTNAME" ]; then
            info "使用自定义 hostname: ${IRIS_HOSTNAME}"
            env_vars="Environment=\"IRIS_HOSTNAME=${IRIS_HOSTNAME}\""
        fi

        # 尝试使用 systemd 启动
        if ! setup_systemd_service "iris-agent" "iris-agent" "--server ${IRIS_SERVER}" "$env_vars" ""; then
            # 没有 systemd 或没有权限，显示手动运行提示
            echo ""
            warning "无法创建 systemd 服务，请手动启动 agent:"
            if [ -n "$IRIS_HOSTNAME" ]; then
                echo -e "  ${GREEN}IRIS_HOSTNAME=${IRIS_HOSTNAME} iris-agent --server ${IRIS_SERVER}${NC}"
            else
                echo -e "  ${GREEN}iris-agent --server ${IRIS_SERVER}${NC}"
            fi
            echo ""
        fi
    else
        # server 模式
        info "未设置 IRIS_SERVER，安装 server 模式"
        install_binary "iris-server" "$platform"

        # 尝试创建数据目录（生产环境持久化）
        local data_dir="/var/lib/iris"
        local has_data_dir=false
        if can_sudo_or_root; then
            local prefix=$(get_prefix_cmd)
            info "创建数据目录: ${data_dir}"
            if ${prefix} mkdir -p "$data_dir" 2>/dev/null; then
                # 如果不是 root，设置目录权限给当前用户
                if [ "$(id -u)" != "0" ]; then
                    ${prefix} chown "$(whoami)" "$data_dir" 2>/dev/null || true
                fi
                success "数据将持久化到 ${data_dir}"
                has_data_dir=true
            else
                warning "无法创建 ${data_dir}"
            fi
        else
            warning "无 sudo 权限，无法创建 ${data_dir}"
        fi

        if [ "$has_data_dir" = false ]; then
            warning "数据不会持久化（仅内存模式），重启后数据将丢失"
        fi

        # 尝试使用 systemd 启动
        local working_dir=""
        if [ "$has_data_dir" = true ]; then
            working_dir="/var/lib/iris"
        fi

        if ! setup_systemd_service "iris-server" "iris-server" "--addr 0.0.0.0:50051" "" "$working_dir"; then
            # 没有 systemd 或没有权限，显示手动运行提示
            echo ""
            warning "无法创建 systemd 服务，请手动启动 server:"
            echo -e "  ${GREEN}iris-server --addr 0.0.0.0:50051${NC}"
            echo ""
            echo -e "在其他机器上安装 agent:"
            echo -e "  ${YELLOW}curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | IRIS_SERVER=http://<server-ip>:50051 bash${NC}"
            echo ""
        fi
    fi

    echo ""
    success "安装完成！"
    echo ""

    # 显示管理命令
    if has_systemd; then
        local service_name="iris-server"
        [ -n "$IRIS_SERVER" ] && service_name="iris-agent"

        local prefix=$(get_prefix_cmd)
        if [ -z "$prefix" ]; then
            prefix=""
        else
            prefix="${prefix} "
        fi

        echo -e "管理命令:"
        echo -e "  查看状态: ${YELLOW}${prefix}systemctl status ${service_name}${NC}"
        echo -e "  查看日志: ${YELLOW}${prefix}journalctl -u ${service_name} -f${NC}"
        echo -e "  重启服务: ${YELLOW}${prefix}systemctl restart ${service_name}${NC}"
        echo -e "  停止服务: ${YELLOW}${prefix}systemctl stop ${service_name}${NC}"
        echo ""
    fi

    # 检查 PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        warning "${INSTALL_DIR} 不在 PATH 中"
        echo "  请将以下内容添加到 ~/.bashrc 或 ~/.zshrc:"
        echo -e "    ${GREEN}export PATH=\"\$PATH:${INSTALL_DIR}\"${NC}"
    fi
}

# 处理参数
case "${1:-}" in
    -h|--help)
        show_usage
        exit 0
        ;;
    *)
        main "$@"
        ;;
esac
