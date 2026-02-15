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
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
WEB_DIR="${WEB_DIR:-/opt/iris/web}"
VERSION="${VERSION:-latest}"
INSTALL_AGENT="${INSTALL_AGENT:-true}"
INSTALL_SERVER="${INSTALL_SERVER:-true}"
GITHUB_PROXY="${GITHUB_PROXY:-}"  # 可选的 GitHub 代理

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

    # 检查安装目录权限
    if [ ! -w "$INSTALL_DIR" ]; then
        warning "需要 sudo 权限安装到 ${INSTALL_DIR}"
        sudo install -m 755 "$binary_file" "${INSTALL_DIR}/${binary_name}${ext}"
    else
        install -m 755 "$binary_file" "${INSTALL_DIR}/${binary_name}${ext}"
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

    # 创建 Web 目录
    if [ ! -w "$(dirname "$WEB_DIR")" ]; then
        warning "需要 sudo 权限安装到 ${WEB_DIR}"
        sudo mkdir -p "$WEB_DIR"
        sudo cp -r web/* "$WEB_DIR/"
    else
        mkdir -p "$WEB_DIR"
        cp -r web/* "$WEB_DIR/"
    fi

    # 清理临时文件
    cd - > /dev/null
    rm -rf "$tmp_dir"

    success "Web UI 已安装到 ${WEB_DIR}"
}

# 显示使用说明
show_usage() {
    cat << EOF
Iris 一键安装脚本

用法:
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash

环境变量:
  VERSION         指定版本号（默认: latest）
  INSTALL_DIR     安装目录（默认: /usr/local/bin）
  INSTALL_AGENT   是否安装 agent（默认: true）
  INSTALL_SERVER  是否安装 server（默认: true）

示例:
  # 安装最新版本
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash

  # 安装指定版本
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | VERSION=v0.1.0 bash

  # 只安装 agent
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | INSTALL_SERVER=false bash

  # 安装到自定义目录
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | INSTALL_DIR=~/.local/bin bash

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

    # 安装二进制文件
    if [ "$INSTALL_AGENT" = "true" ]; then
        install_binary "iris-agent" "$platform" "$VERSION"
    fi

    if [ "$INSTALL_SERVER" = "true" ]; then
        install_binary "iris-server" "$platform" "$VERSION"
        # 安装 Web UI（仅当安装 server 时）
        install_web_ui "$VERSION"
    fi

    echo ""
    success "安装完成！"
    echo ""

    # 显示安装的二进制文件
    if [ "$INSTALL_AGENT" = "true" ]; then
        echo -e "  ${BLUE}iris-agent${NC} -> ${INSTALL_DIR}/iris-agent"
        echo -e "    运行: ${GREEN}iris-agent --server http://your-server:50051${NC}"
    fi

    if [ "$INSTALL_SERVER" = "true" ]; then
        echo -e "  ${BLUE}iris-server${NC} -> ${INSTALL_DIR}/iris-server"
        echo -e "    运行: ${GREEN}iris-server --addr 0.0.0.0:50051${NC}"
        echo -e "  ${BLUE}Web UI${NC} -> ${WEB_DIR}"
        echo -e "    自定义: ${YELLOW}编辑 ${WEB_DIR}/index.html${NC}"
    fi

    echo ""

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
