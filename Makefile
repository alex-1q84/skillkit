# skillkit Makefile（对标 Java monorepo Makefile：setup/format/lint/test/build 统一入口）
# CI 与本地走同一套规则，避免「本地过 CI 不过」。
.PHONY: setup format lint test build check run

## 安装/拉取依赖
setup:
	cargo fetch

## 格式化代码（apply，会改源码）
format:
	cargo fmt --all

## 静态检查（read-only）：格式校验 + clippy，-D warnings 视为错误
lint:
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings

## 跑全部测试
test:
	cargo test --all

## 编译（release 之外的日常构建）
build:
	cargo build --all

## 提交前一站式检查：先格式化，再 lint，再测试
check: format lint test

## 运行 CLI（最新源码，避免 make check 后拿到旧 bin）：make run ARGS="source list"
run:
	cargo run -p skillkit-cli -- $(ARGS)
