# skillkit

AI agent skill 的统一管理工具。设定安装源、记录并锁定版本、按 profile 组织候选集、按项目精确安装并幂等落地到各 agent 目录。

Rust 单二进制，CLI（供 AI agent 高频调用）+ 本地 web GUI（供人总览配置）共享同一 core，零运行时依赖。

## 安装

```bash
cargo install --path crates/cli
```

## 快速开始

```bash
# 添加安装源
skillkit source add owner/repo
skillkit source add ./local-skills --name my-local

# 安装 skill（固定源直接装，skills.sh 源交互选候选）
skillkit install add owner/repo pdf
skillkit install add skills.sh pdf

# 查看项目状态并落地
skillkit project status
skillkit project apply

# 启动 web GUI
skillkit serve
```

## 命令参考

### source — 安装源管理

```bash
skillkit source add <package> [--name <别名>]   # github shorthand / git url / local path
skillkit source list
skillkit source remove <name>
```

### install — 安装 skill

```bash
skillkit install add <source> <skill> [--scope global|local] [--json]
```

- 固定源直接安装；skills.sh（registry）源走 `npx skills find` 交互选候选
- `--json` 时只输出候选数组，不交互不安装（适合 AI agent 调用）

### project — 项目管理

```bash
skillkit project add <path> [--name <名称>]
skillkit project status                           # 查看 diff（该有/缺/多/冲突）
skillkit project apply                           # 幂等落地到 agent 目录
skillkit project list
```

### profile — 候选集管理

```bash
skillkit profile create <name>
skillkit profile add-skill <profile> <id>        # id = <source>/<skill>
skillkit profile remove-skill <profile> <id>
skillkit profile list
```

### upgrade — 版本升级

```bash
skillkit upgrade <id>                            # 升级单个 skill
skillkit upgrade --all [--yes]                   # 批量升级，--yes 跳过冲突确认
```

### import-existing — 导入存量 skill

```bash
skillkit import-existing                         # 扫描存量目录，可溯重装入池 + 无源登记
skillkit import-existing --dry-run               # 只输出不写
skillkit import-existing --json                  # JSON 输出
```

### uninstall — 卸载

```bash
skillkit uninstall <id>                          # 从 canonical 池移除
```

### serve — Web GUI

```bash
skillkit serve [--port 7317] [--no-open]          # 四视图 + apply 闭环 + SSE
```

## 支持的 Agent

| Agent | 安装方式 | 说明 |
|-------|---------|------|
| Claude Code | symlink（双层桥接） | global scope 时自动创建 |
| Cursor | symlink | 直读 `~/.agents/skills/` |
| OpenCode | symlink | 直读 |
| Codex | copy | 无法 symlink 时的 fallback |

新增 agent 只需配置，不改代码。详见 `~/.skillkit/config.toml`。

## 开发

```bash
make check          # format + lint + test（提交前一站式）
make build          # 编译
make test           # 全量测试
make run ARGS="..." # 跑 CLI
make e2e            # GUI 端到端（playwright + chromium）
make e2e-cli        # CLI 全链路端到端（assert_cmd + npx skills）
```

## 架构

```
crates/
  core/    # skillkit-core（lib）— 全部业务逻辑
  cli/     # skillkit-cli（bin）— 薄壳调 core
  server/  # skillkit-server（lib）— Axum + Askama + htmx
```

CLI 和 server 都是 core 的薄壳，不允许出现重复业务逻辑。状态存储统一在 `~/.skillkit/`，并发写用文件锁串行化。

## License

MIT
