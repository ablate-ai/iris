.PHONY: help build build-agent build-server build-all test clean fmt clippy run-server run-agent dev release release-major release-minor release-patch

# 默认目标
help:
	@echo "Iris 项目 Makefile"
	@echo ""
	@echo "可用命令:"
	@echo "  make build          - 编译所有二进制（debug 模式）"
	@echo "  make build-agent    - 编译 iris-agent"
	@echo "  make build-server   - 编译 iris-server"
	@echo "  make build-all      - 编译所有二进制（release 模式）"
	@echo "  make test           - 运行测试"
	@echo "  make fmt            - 格式化代码"
	@echo "  make clippy         - 运行 clippy 检查"
	@echo "  make clean          - 清理构建产物"
	@echo "  make run-server     - 运行 server（开发模式）"
	@echo "  make run-agent      - 运行 agent（开发模式）"
	@echo "  make dev            - 一键启动 server 和 agent（开发模式）"
	@echo "  make release        - 创建并推送 release tag（需指定 VERSION）"
	@echo "  make release-major  - 自动递增主版本号 (x.0.0)"
	@echo "  make release-minor  - 自动递增次版本号 (x.y.0)"
	@echo "  make release-patch  - 自动递增补丁版本号 (x.y.z)"

# 编译（debug 模式）
build:
	cargo build --bin iris-agent
	cargo build --bin iris-server

# 编译 agent
build-agent:
	cargo build --release --bin iris-agent

# 编译 server
build-server:
	cargo build --release --bin iris-server

# 编译所有（release 模式）
build-all:
	cargo build --release --bin iris-agent
	cargo build --release --bin iris-server

# 运行测试
test:
	cargo test

# 格式化代码
fmt:
	cargo fmt --all

# Clippy 检查
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

# 清理
clean:
	cargo clean

# 运行 server（开发模式）
run-server:
	cargo run --bin iris-server -- --addr 0.0.0.0:50051

# 运行 agent（开发模式）
run-agent:
	cargo run --bin iris-agent -- --server http://127.0.0.1:50051 --interval 1

# 一键启动开发环境（同时运行 server 和 agent）
dev:
	@echo "🚀 启动开发环境..."
	@echo "📍 Web UI: http://localhost:50052"
	@echo "📍 gRPC: localhost:50051"
	@echo "📍 按 Ctrl+C 停止所有服务"
	@echo ""
	@trap 'kill 0' EXIT; \
	cargo run --bin iris-server -- --addr 0.0.0.0:50051 & \
	sleep 3; \
	cargo run --bin iris-agent -- --server http://127.0.0.1:50051 --interval 1

# 创建 release
release:
	@if [ -z "$(VERSION)" ]; then \
		echo "错误: 请指定版本号"; \
		echo "用法: make release VERSION=0.1.0"; \
		exit 1; \
	fi
	@./scripts/release.sh $(VERSION)

# 自动递增主版本号 (x.0.0)
release-major:
	@./scripts/bump-version.sh major

# 自动递增次版本号 (x.y.0)
release-minor:
	@./scripts/bump-version.sh minor

# 自动递增补丁版本号 (x.y.z)
release-patch:
	@./scripts/bump-version.sh patch
