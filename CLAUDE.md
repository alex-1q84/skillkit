# SkillKit — 项目规范

> 本文件是**代码层规范**（怎么写代码）。设计意图与决策推理的唯一权威是 `docs/2026-07-29-skillkit-design.md`（spec）和 `docs/design-decisions-2026-07-29.md`（决策纪要）。规范与设计冲突时先改文档、再改实践，不要反过来。

## 1. 项目定位

skillkit 是 AI agent skill 的统一管理工具：设定安装源、记录并锁定版本、按 profile 组织候选集、按项目精确安装并幂等落地到各 agent 目录。Rust 单二进制，CLI（供 AI agent 高频调用）+ 本地 web GUI（供人总览配置）共享同一 core，前端产物嵌入二进制，零运行时依赖。

## 2. 技术栈

- Rust（edition 2021）+ Axum，单二进制。
- 前端 htmx + Askama（服务端渲染片段）+ SortableJS（拖拽），静态资源经 `rust-embed` 嵌入二进制，无独立前端工程。
- **前端不强制零 JS**：可用轻量原生 JS / htmx 增强交互（如实时预览、事件互斥）；但**禁止 React / Vue 等重型前端框架**，不引入 node 构建链，保持单二进制零运行时依赖。
- 分发：纳入 mac-config Brewfile（与 cx/rtk 工具链一致）。

## 3. 架构（三层共享 core）

- `core` crate 承载**全部业务逻辑**，CLI 和 server 都只是薄壳，不允许出现重复业务逻辑。
- 进程模型：**无常驻 daemon**。CLI 直接调 core；`skillkit serve` 启 Axum 也直接调 core。状态实时性靠 core 写状态文件 + web 端 SSE 推送。
- 并发写：CLI 与 server 同时写 `~/.skillkit/` 时用文件锁（`~/.skillkit/.lock`）串行化。

## 4. 目录结构（Cargo workspace）

```
skillkit/
  Cargo.toml              # workspace 根
  crates/
    core/                 # package: skillkit-core（lib）—— 全部业务逻辑
    cli/                  # package: skillkit-cli，binary name: skillkit —— 薄壳调 core
    server/               # package: skillkit-server（M2 才创建）—— Axum + Askama 模板 + rust-embed 静态
  docs/                   # 设计文档 + sessions/ 交接材料
  CLAUDE.md
```

M0 只建 `crates/core` + `crates/cli`。`server` 到 M2 再加，避免空目录。

## 5. 不可违反的约束（代码层硬约束）

这几条是设计层面的不变量，代码里任何时候都不能破坏：

- **`~/.agents/skills/` 只放全局公共 skill**：绝不挪用为项目级暂存，绝不往里写 `.registry.json` 之类元数据文件（会被 Cursor/OpenCode/Codex/Gemini 等误扫描）。违反即污染通用加载目录。
- **元数据统一收 `~/.skillkit/`**：config / sources / registry / profiles / projects / lock 全在此目录下。
- **单版本模型**：canonical 物理存储只有一份，版本信息记在 registry 的 `computed_hash` 和 project 的 `locked_shas` 里（源自 npx skills 的 skills-lock.json）。不为多版本预先分目录（YAGNI，spec §16 预留升级路径）。
- **项目 shared skill 只读发现**：扫描展示即可，不安装/升级/卸载——它由项目 git 自己管。
- **跨实体用 id 引用**：id = `<source-name>/<skill-name>`，source/scope/version 等信息只在 registry 存一份，profile/project 只存 id 列表（DRY）。
- **agent 能力配置驱动**：是否支持 symlink、是否直读 `~/.agents/skills/` 声明在 `config.toml`，新增 agent 只改配置不改代码。`Config::default()` 默认声明 claude-code/cursor/codex 三大主流 agent（开箱覆盖 `.claude`/`.cursor`/`.codex`），其余 agent 按需追加。**项目 agents 精确探测**：注册/绑定/同步时按配置目录（`.claude`/`.codex`/`.cursor`/`.agents`）→ 指令文件（`CLAUDE.md`/`AGENTS.md`）判定实际使用的 agent，全部未命中回退开源标准 `.agents/`，绝不默认给全 agent 建目录；旧项目用 `sync-agents` 一键校正。`scan_shared` 在 `proj.agents` 各 agent 目录之外，额外只读发现项目级 `.agents/skills/` 共享池（归属 `agents/<name>`，不参与 apply 落地）。
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
- **core 公开类型一律在 `lib.rs` 完整 re-export**：子模块定义的 pub 类型若不 re-export，crate 内（`crate::T`）和外部（`skillkit_core::T`）都找不到。每 crate 选一种约定统一（短路径 re-export 或全模块路径），不混用——混用是漏 re-export 的温床，只在后续模块编译时才暴露。
- 命名、注释跟随 Rust 惯例；注释用中文，与文档和 commit 语言一致。

## 7.5 前端约定（server crate）

前端规则细化见 `docs/frontend-rules.md`（类比 project-initialization 按语言给 AI 约束）。核心强规则：

| 必须 | 禁止 |
|------|------|
| 前端交互优先 htmx 服务端渲染 + 片段 | React / Vue 等重型框架；node 构建链；npm 依赖 |
| 业务逻辑只在 core，server handler 是薄壳 | 在 handler/template 复制 core 的推导/计算逻辑 |
| SSE 刷新请求 `?fragment=1` 纯片段（响应不含 nav） | SSE 刷新返回完整页再 select 提取（曾致导航重复） |
| 写操作（POST/DELETE）返回完整页面 `hx-target="body" hx-swap="outerHTML"` | 写操作返回片段却用 body outerHTML |
| 片段外层固定 id（局部替换后 id 不丢） | 片段外层 id 随内容变化 |
| 页面模板薄壳 + include fragment（main 内容只在 fragment） | 页面模板重复 main 内容 |
| 前端推导规则只在 core（htmx 调服务端点） | 前端复制一份推导逻辑 |
| 改模板/静态资源后跑 `make check` | 只改不验（Askama 模板错只有 check 能暴露） |

Askama/htmx 具体坑（match 头花括号歧义、include 不传变量、重复 key 表单、`%2F` 编码、尾斜杠 404、原生 EventSource 等）见 `docs/frontend-rules.md` §4。

## 8. 测试约定

核心原则：**测试验证业务结果（apply 后项目能加载到正确 skill），不验证实现细节（调了哪个内部函数）**。业务逻辑变了测试应失败，否则就是测错了。

- 单元测试：core 纯逻辑（registry 解析、profile 合并、diff 计算、冲突检测、id/project-id 生成）。
- 集成测试：`tempfile::tempdir()` 模拟整个 `~/.skillkit` + `~/.agents` + 项目目录，跑 install → apply 全流程，断言 symlink/copy 正确落地。
- 多 agent 路径分别覆盖：Claude（symlink）和 Cursor（copy）两条。
- 幂等测试：重复 apply 断言零变化。
- 冲突场景：多项目锁不同版本、dangling symlink、源失效。
- `--json` schema 锁定测试：防 agent 依赖的结构被无意改动。
- git 操作用本地 bare repo 真跑，不 mock。
- **集成测试放对应 crate 的 `tests/`**（如 `crates/core/tests/`），不放 workspace 根——纯 workspace 根（无 `[package]`）的 `tests/` 被 cargo test 静默忽略。
- **测试里跑 `git commit` 必须带 `-c user.email -c user.name`**，不依赖机器全局 git config（换环境稳定，避免 commit 静默失败）。

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
make run ARGS="..."  # 跑最新 CLI（check 只 clippy check + test，不产出独立 bin；直接跑 target/debug/skillkit 会拿到旧 bin）
make e2e             # GUI 端到端（python playwright + chromium，真实浏览器；需空闲端口，不进 check）
make e2e-cli         # CLI 全链路端到端（assert_cmd 驱动真实二进制 + 真跑 npx skills；不进 check）
```

## 10. Commit 规范

- Conventional Commits（`feat:` / `fix:` / `chore:` / `docs:` / `test:` / `refactor:`），message 用中文。
- **不自动 git**：未获主人明确指示前不执行 add/commit/branch/push。遵循主人全局习惯。
