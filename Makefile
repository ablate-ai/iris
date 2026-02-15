.PHONY: help build build-agent build-server build-all test clean fmt clippy run-server run-agent release

# 默认目标
help:
	@echo "Iris 项目 Makefile"
	@echo ""
	@echo "可用命令:"
	@echo "  make build         - 编译所有二进制（debug 模式）"
	@echo "  make build-agent   - 编译 iris-agent"
	@echo "  make build-server  - 编译 iris-server"
	@echo "  make build-all     - 编译所有二进制（release 模式）"
	@echo "  make test          - 运行测试"
	@echo "  make fmt           - 格式化代码"
	@echo "  make clippy        - 运行 clippy 检查"
	@echo "  make clean         - 清理构建产物"
	@echo "  make run-server    - 运行 server（开发模式）"
	@echo "  make run-agent     - 运行 agent（开发模式）"
	@echo "  make release       - 创建并推送 release tag"

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
	cargo run --bin iris-agent -- --server http://127.0.0.1:50051 --interval 10

# 创建 release
release:
	@if [ -z "$(VERSION)" ]; then \
		echo "错误: 请指定版本号"; \
		echo "用法: make release VERSION=0.1.0"; \
		exit 1; \
	fi
	@./scripts/release.sh $(VERSION)
