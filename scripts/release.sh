#!/bin/bash
set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查参数
if [ $# -eq 0 ]; then
    echo -e "${RED}错误: 请提供版本号${NC}"
    echo "用法: $0 <version>"
    echo "示例: $0 0.1.0"
    exit 1
fi

VERSION=$1

# 验证版本号格式 (x.y.z)
if ! [[ $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo -e "${RED}错误: 版本号格式不正确${NC}"
    echo "版本号应该是 x.y.z 格式，例如: 0.1.0"
    exit 1
fi

TAG="v${VERSION}"

# 检查是否有未提交的修改
if ! git diff-index --quiet HEAD --; then
    echo -e "${RED}错误: 有未提交的修改，请先提交或暂存${NC}"
    git status --short
    exit 1
fi

# 检查 tag 是否已存在
if git rev-parse "$TAG" >/dev/null 2>&1; then
    echo -e "${RED}错误: Tag $TAG 已存在${NC}"
    exit 1
fi

echo -e "${YELLOW}准备发布版本: $TAG${NC}"
echo ""

# 确认
read -p "确认创建并推送 tag $TAG? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}已取消${NC}"
    exit 0
fi

# 更新所有 Cargo.toml 的版本号
echo -e "${GREEN}更新 Cargo.toml 版本号...${NC}"
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" agent/Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" server/Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" common/Cargo.toml

# 删除备份文件
rm -f Cargo.toml.bak agent/Cargo.toml.bak server/Cargo.toml.bak common/Cargo.toml.bak

# 提交版本号变更
echo -e "${GREEN}提交版本号变更...${NC}"
git add Cargo.toml agent/Cargo.toml server/Cargo.toml common/Cargo.toml
git commit -m "chore: bump version to ${VERSION}"

# 创建 tag
echo -e "${GREEN}创建 tag: $TAG${NC}"
git tag -a "$TAG" -m "Release $TAG"

# 推送提交和 tag
echo -e "${GREEN}推送到远程仓库...${NC}"
git push origin main
git push origin "$TAG"

echo ""
echo -e "${GREEN}✓ 发布成功!${NC}"
echo -e "GitHub Actions 将自动构建并创建 Release"
echo -e "查看进度: https://github.com/$(git remote get-url origin | sed 's/.*github.com[:/]\(.*\)\.git/\1/')/actions"
