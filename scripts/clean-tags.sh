#!/bin/bash

# 清理 Git Tags 脚本
# 用途：删除本地和远程的所有 tags

set -e

echo "🔍 正在获取所有 tags..."
tags=$(git tag -l)

if [ -z "$tags" ]; then
    echo "✅ 没有找到任何 tags"
    exit 0
fi

echo "📋 找到以下 tags:"
echo "$tags"
echo ""

# 确认操作
read -p "⚠️  确定要删除所有 tags 吗？(y/N): " confirm
if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
    echo "❌ 操作已取消"
    exit 0
fi

echo ""
echo "🗑️  开始删除本地 tags..."
for tag in $tags; do
    git tag -d "$tag"
    echo "  ✓ 已删除本地 tag: $tag"
done

echo ""
echo "🌐 开始删除远程 tags..."
for tag in $tags; do
    if git ls-remote --tags origin | grep -q "refs/tags/$tag"; then
        git push origin ":refs/tags/$tag"
        echo "  ✓ 已删除远程 tag: $tag"
    else
        echo "  ⊘ 远程不存在 tag: $tag"
    fi
done

echo ""
echo "✅ 所有 tags 已清理完成！"
