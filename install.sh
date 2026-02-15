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
WEB_DIR="/opt/iris/web"
VERSION="${VERSION:-latest}"

# k3s 风格：配置了 IRIS_SERVER 就是 agent，否则是 server
IRIS_SERVER="${IRIS_SERVER:-}"

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

# 创建并启动 systemd 服务
setup_systemd_service() {
    local service_name=$1
    local binary_name=$2
    local exec_args=$3

    if ! has_systemd; then
        return 1
    fi

    info "检测到 systemd，创建服务: ${service_name}"

    # 创建 systemd service 文件
    warning "需要 sudo 权限创建 systemd 服务"
    sudo tee "/etc/systemd/system/${service_name}.service" > /dev/null <<EOF
[Unit]
Description=Iris ${binary_name}
After=network.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/${binary_name} ${exec_args}
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

    # 重载并启动服务
    info "重载 systemd 并启动 ${service_name}..."
    sudo systemctl daemon-reload
    sudo systemctl enable "${service_name}"
    sudo systemctl restart "${service_name}"

    # 等待启动
    sleep 2

    if systemctl is-active --quiet "${service_name}"; then
        success "${service_name} 已启动"
        return 0
    else
        error "${service_name} 启动失败，请查看日志: sudo journalctl -u ${service_name} -n 50"
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

# 获取最新版本号
get_latest_version() {
    info "获取最新版本..." >&2
    local version
    local api_url="${GITHUB_PROXY}https://api.github.com/repos/${REPO}/releases/latest"

    version=$(curl -fsSL -H "User-Agent: iris-installer" "$api_url" 2>/dev/null | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

    if [ -z "$version" ]; then
        warning "无法从 GitHub API 获取版本号，尝试备用方法..."
        # 备用方法：从 GitHub releases 页面获取
        local releases_url="${GITHUB_PROXY}https://github.com/${REPO}/releases/latest"
        version=$(curl -fsSL -H "User-Agent: iris-installer" "$releases_url" 2>/dev/null | grep -oP 'tag/\K[^"]+' | head -1)
    fi

    if [ -z "$version" ]; then
        error "无法获取最新版本号，请手动指定版本: VERSION=v0.1.0 bash install.sh"
    fi

    echo "$version"
}

# 下载并安装
install_binary() {
    local binary_name=$1
    local platform=$2
    local version=$3

    info "安装 ${binary_name}..."

    # 构建下载 URL
    local ext=""
    if [[ "$platform" == "windows-"* ]]; then
        ext=".exe"
        local archive_name="iris-${platform}.zip"
    else
        local archive_name="iris-${platform}.tar.gz"
    fi

    local download_url="${GITHUB_PROXY}https://github.com/${REPO}/releases/download/${version}/${archive_name}"
    local tmp_dir=$(mktemp -d)

    info "下载 ${archive_name}..."
    if ! curl -fsSL -H "User-Agent: iris-installer" "$download_url" -o "${tmp_dir}/${archive_name}"; then
        error "下载失败: ${download_url}"
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

# 下载并安装 Web UI
install_web_ui() {
    local version=$1

    info "安装 Web UI..."

    # 构建下载 URL
    local download_url="${GITHUB_PROXY}https://github.com/${REPO}/releases/download/${version}/web.tar.gz"
    local tmp_dir=$(mktemp -d)

    info "下载 web.tar.gz..."
    if ! curl -fsSL -H "User-Agent: iris-installer" "$download_url" -o "${tmp_dir}/web.tar.gz"; then
        warning "下载 Web UI 失败，跳过"
        rm -rf "$tmp_dir"
        return 0
    fi

    # 解压
    info "解压 Web UI..."
    cd "$tmp_dir"
    tar -xzf web.tar.gz

    # 检测解压后的目录结构
    if [ -d "web" ]; then
        # 如果有 web 目录，使用其内容
        SRC_DIR="web"
    else
        # 否则直接使用当前目录
        SRC_DIR="."
    fi

    # 创建 Web 目录并复制文件
    if ! mkdir -p "$WEB_DIR" 2>/dev/null; then
        warning "需要 sudo 权限安装到 ${WEB_DIR}"
        sudo mkdir -p "$WEB_DIR"
        sudo cp -r "$SRC_DIR"/* "$WEB_DIR/"
    else
        cp -r "$SRC_DIR"/* "$WEB_DIR/"
    fi

    # 清理临时文件
    cd - > /dev/null
    rm -rf "$tmp_dir"

    success "Web UI 已安装到 ${WEB_DIR}"
}

# 显示使用说明
show_usage() {
    cat << EOF
Iris 一键安装脚本 (k3s 风格)

用法:
  # 安装 server
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash

  # 安装 agent
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | IRIS_SERVER=http://192.168.1.100:50051 bash

环境变量:
  IRIS_SERVER     server 地址（设置此值则安装 agent，否则安装 server）
  VERSION         指定版本号（默认: latest）

安装位置:
  二进制文件:     /usr/local/bin
  Web UI:         /opt/iris/web

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

    # 获取版本
    if [ "$VERSION" = "latest" ]; then
        VERSION=$(get_latest_version)
    fi
    info "安装版本: ${VERSION}"

    # 创建安装目录
    if [ ! -d "$INSTALL_DIR" ]; then
        warning "创建安装目录: ${INSTALL_DIR}"
        mkdir -p "$INSTALL_DIR" || sudo mkdir -p "$INSTALL_DIR"
    fi

    # 判断安装模式
    if [ -n "$IRIS_SERVER" ]; then
        # agent 模式
        info "检测到 IRIS_SERVER，安装 agent 模式"
        install_binary "iris-agent" "$platform" "$VERSION"

        # 尝试使用 systemd 启动
        if ! setup_systemd_service "iris-agent" "iris-agent" "--server ${IRIS_SERVER}"; then
            # 没有 systemd 或启动失败，显示手动运行提示
            echo ""
            warning "未检测到 systemd，请手动启动 agent:"
            echo -e "  ${GREEN}iris-agent --server ${IRIS_SERVER}${NC}"
            echo ""
        fi
    else
        # server 模式
        info "未设置 IRIS_SERVER，安装 server 模式"
        install_binary "iris-server" "$platform" "$VERSION"
        # 安装 Web UI
        install_web_ui "$VERSION"

        # 尝试使用 systemd 启动
        if ! setup_systemd_service "iris-server" "iris-server" "--addr 0.0.0.0:50051 --web-dir ${WEB_DIR}"; then
            # 没有 systemd 或启动失败，显示手动运行提示
            echo ""
            warning "未检测到 systemd，请手动启动 server:"
            echo -e "  ${GREEN}iris-server --addr 0.0.0.0:50051 --web-dir ${WEB_DIR}${NC}"
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

        echo -e "管理命令:"
        echo -e "  查看状态: ${YELLOW}sudo systemctl status ${service_name}${NC}"
        echo -e "  查看日志: ${YELLOW}sudo journalctl -u ${service_name} -f${NC}"
        echo -e "  重启服务: ${YELLOW}sudo systemctl restart ${service_name}${NC}"
        echo -e "  停止服务: ${YELLOW}sudo systemctl stop ${service_name}${NC}"
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
