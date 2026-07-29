# SkillKit — 项目规范

> 本文件是**代码层规范**（怎么写代码）。设计意图与决策推理的唯一权威是 `docs/2026-07-29-skillkit-design.md`（spec）和 `docs/design-decisions-2026-07-29.md`（决策纪要）。规范与设计冲突时先改文档、再改实践，不要反过来。

## 1. 项目定位

skillkit 是 AI agent skill 的统一管理工具：设定安装源、记录并锁定版本、按 profile 组织候选集、按项目精确安装并幂等落地到各 agent 目录。Rust 单二进制，CLI（供 AI agent 高频调用）+ 本地 web GUI（供人总览配置）共享同一 core，前端产物嵌入二进制，零运行时依赖。

## 2. 技术栈

- Rust（edition 2021）+ Axum，单二进制。
- 前端 React + Vite，构建产物经 `rust-embed` 嵌入二进制。
- 分发：纳入 mac-config Brewfile（与 cx/rtk 工具链一致）。

## 3. 架构（三层共享 core）

- `core` crate 承载**全部业务逻辑**，CLI 和 server 都只是薄壳，不允许出现重复业务逻辑。
- 进程模型：**无常驻 daemon**。CLI 直接调 core；`skillkit serve` 启 Axum 也直接调 core。状态实时性靠 core 写状态文件 + web 端 SSE 推送。
- 并发写：CLI 与 server 同时写 `~/.skm/` 时用文件锁（`~/.skm/.lock`）串行化。

## 4. 目录结构（Cargo workspace）

```
skillkit/
  Cargo.toml              # workspace 根
  crates/
    core/                 # package: skillkit-core（lib）—— 全部业务逻辑
    cli/                  # package: skillkit-cli，binary name: skillkit —— 薄壳调 core
    server/               # package: skillkit-server（M2 才创建）—— Axum + rust-embed
  web/                    # M2 才创建：React + Vite 前端源码
  docs/                   # 设计文档 + sessions/ 交接材料
  CLAUDE.md
```

M0 只建 `crates/core` + `crates/cli`。`server`、`web` 到 M2 再加，避免空目录。

## 5. 不可违反的约束（代码层硬约束）

这几条是设计层面的不变量，代码里任何时候都不能破坏：

- **`~/.agents/skills/` 只放全局公共 skill**：绝不挪用为项目级暂存，绝不往里写 `.registry.json` 之类元数据文件（会被 Cursor/OpenCode/Codex/Gemini 等误扫描）。违反即污染通用加载目录。
- **元数据统一收 `~/.skm/`**：config / sources / registry / profiles / projects / lock 全在此目录下。
- **单版本模型**：canonical 物理存储只有一份，版本信息记在 registry 的 `commit_sha` 和 project 的 `locked_shas` 里。不为多版本预先分目录（YAGNI，spec §16 预留升级路径）。
- **项目 shared skill 只读发现**：扫描展示即可，不安装/升级/卸载——它由项目 git 自己管。
- **跨实体用 id 引用**：id = `<source-name>/<skill-name>`，source/scope/version 等信息只在 registry 存一份，profile/project 只存 id 列表（DRY）。
- **agent 能力配置驱动**：是否支持 symlink、是否直读 `~/.agents/skills/` 声明在 `config.toml`，新增 agent 只改配置不改代码。
- **不管 Claude 原生插件**（`~/.claude/plugins/`）**和 `~/.claude/commands/`**：碰会冲突。
- **项目 local skill 平铺落地**：与 shared 同级落在 `<agent>/skills/<skill>/`，绝不建 `local/` 子目录（Claude Code 只发现一层目录下的 skill，子目录会被跳过）；git 忽略写 `<project>/.git/info/exclude`，不改项目 `.gitignore`。详见 spec §6.3、决策 12。
- **改完源码必跑 `make format && make lint`**：`format` 应用 rustfmt（会改源码）、`lint` 只检查（fmt --check + clippy `-D warnings`）。提交前确保双绿，否则 CI 会拦。详见 §9。

## 6. CLI 约定

- git-style 分组（`source` / `install` / `profile` / `project` / `serve`）。
- **全命令支持 `--json`**，且 `--json` 输出 schema 视为公开契约，AI agent 依赖其稳定，变更需谨慎。
- **幂等可重入**：重复 `apply` 零副作用，agent 可放心重试。
- 危险操作（uninstall / remove / apply 的删除项）默认交互确认，`--yes` 跳过以适配 CI/agent。
- `project status` 是感知接口、`project apply` 是执行接口，二者闭环。

## 7. 代码约定

- **路径绝不硬编码**：用户目录一律用 `dirs::home_dir()` 解析，不写死 `/Users/...`。
- **错误处理**：core crate 用 `thiserror` 定义具体错误类型，让调用方决定呈现方式；cli/server 顶层用 `anyhow` 聚合。错误信息遵循「反馈引导行动」——不只报告失败，要给出下一步（"skill 未安装，先 `skillkit install <id>`"），不静默跳过任何步骤。
- **CLI 解析**用 `clap`（derive 特性）。
- **序列化**用 `serde`：registry/config/profile 用对应格式（json/toml），结构体 `#[derive(Serialize, Deserialize)]`。
- **日志**用 `tracing`，不裸 `println!`。
- **文件原子写**：写 registry/projects 等状态文件用「写临时文件 + rename」保证原子性。
- 命名、注释跟随 Rust 惯例；注释用中文，与文档和 commit 语言一致。

## 8. 测试约定

核心原则：**测试验证业务结果（apply 后项目能加载到正确 skill），不验证实现细节（调了哪个内部函数）**。业务逻辑变了测试应失败，否则就是测错了。

- 单元测试：core 纯逻辑（registry 解析、profile 合并、diff 计算、冲突检测、id/project-id 生成）。
- 集成测试：`tempfile::tempdir()` 模拟整个 `~/.skm` + `~/.agents` + 项目目录，跑 install → apply 全流程，断言 symlink/copy 正确落地。
- 多 agent 路径分别覆盖：Claude（symlink）和 Cursor（copy）两条。
- 幂等测试：重复 apply 断言零变化。
- 冲突场景：多项目锁不同版本、dangling symlink、源失效。
- `--json` schema 锁定测试：防 agent 依赖的结构被无意改动。
- git 操作用本地 bare repo 真跑，不 mock。

## 9. 常用命令

```bash
cargo build                          # 编译
cargo test                           # 全量测试
cargo test -p skillkit-core          # 只测 core
cargo run -p skillkit-cli -- <cmd>   # 跑 CLI（M0 起可用）
cargo run -p skillkit-server         # 起 web server（M2）
```

日常走 Makefile 统一入口（CI 与本地同规则）：

```bash
make setup        # 拉取依赖
make format       # 应用 rustfmt（会改源码）
make lint         # 格式校验 + clippy -D warnings（read-only）
make test         # 全量测试
make build        # 编译
make check        # 提交前一站式：format && lint && test
```

## 10. Commit 规范

- Conventional Commits（`feat:` / `fix:` / `chore:` / `docs:` / `test:` / `refactor:`），message 用中文。
- **不自动 git**：未获主人明确指示前不执行 add/commit/branch/push。遵循主人全局习惯。
