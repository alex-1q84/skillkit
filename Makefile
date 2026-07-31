# skillkit Makefile（对标 Java monorepo Makefile：setup/format/lint/test/build 统一入口）
# CI 与本地走同一套规则，避免「本地过 CI 不过」。
.PHONY: setup format lint test build check run e2e e2e-cli

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

# --- e2e（真实浏览器驱动 GUI，不进 check；需 chromium + pipx python playwright）---
PY        := /Users/mywo/.local/pipx/venvs/playwright/bin/python
E2E_PORT  := 7417      # 避开主人日常 7317
E2E_TOKEN := e2e-test
E2E_BASE  := http://127.0.0.1:7417/e2e-test/

## 端到端测试：起 serve（固定 token + 临时 HOME 隔离）→ 浏览器跑用例 → 清理
e2e:
	cargo build -p skillkit-cli
	@HOME_TMP=$$(mktemp -d /tmp/skillkit-e2e.XXXXXX); \
	cleanup() { [ -n "$$PID_UI" ] && kill $$PID_UI 2>/dev/null; rm -rf "$$HOME_TMP"; }; \
	trap cleanup EXIT; \
	HOME="$$HOME_TMP" ./target/debug/skillkit serve --port $(E2E_PORT) --no-open --token $(E2E_TOKEN) & PID_UI=$$!; \
	$(PY) e2e/test_ui.py --base "$(E2E_BASE)" --home "$$HOME_TMP"

## CLI 全链路端到端（assert_cmd 驱动真实二进制，含 #[ignore] 真跑 npx skills；不进 check）
e2e-cli:
	cargo test -p skillkit-cli --test e2e_cli -- --ignored
