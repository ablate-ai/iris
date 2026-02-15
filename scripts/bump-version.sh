#!/bin/bash
set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 检查参数
if [ $# -eq 0 ]; then
    echo -e "${RED}错误: 请指定版本类型${NC}"
    echo "用法: $0 <major|minor|patch>"
    exit 1
fi

BUMP_TYPE=$1

# 验证参数
if [[ ! "$BUMP_TYPE" =~ ^(major|minor|patch)$ ]]; then
    echo -e "${RED}错误: 版本类型必须是 major、minor 或 patch${NC}"
    exit 1
fi

# 获取最新的 tag
LATEST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")

# 移除 'v' 前缀
CURRENT_VERSION=${LATEST_TAG#v}

# 解析版本号
if [[ $CURRENT_VERSION =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    MAJOR=${BASH_REMATCH[1]}
    MINOR=${BASH_REMATCH[2]}
    PATCH=${BASH_REMATCH[3]}
else
    echo -e "${RED}错误: 无法解析当前版本号: $CURRENT_VERSION${NC}"
    exit 1
fi

# 根据类型递增版本号
case $BUMP_TYPE in
    major)
        MAJOR=$((MAJOR + 1))
        MINOR=0
        PATCH=0
        ;;
    minor)
        MINOR=$((MINOR + 1))
        PATCH=0
        ;;
    patch)
        PATCH=$((PATCH + 1))
        ;;
esac

NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}"

echo -e "${BLUE}当前版本: ${LATEST_TAG}${NC}"
echo -e "${GREEN}新版本:   v${NEW_VERSION}${NC}"
echo ""

# 调用 release.sh
exec ./scripts/release.sh "$NEW_VERSION"
