# 2026-07-30 skillkit（设计 → review → 工程化 → M0 完成）

> 用途：把 skillkit 工具会话的关键事实、决策、遗留集中沉淀，便于下次在 skillkit 目录启动新会话时接续。
>
> **新会话最快入口**：直接读 §1（当前状态）+ §4（验证清单）+ §7（下次接续路径），三段够用；细节再回查 §2/§3/§5/§6。

## 1. 当前状态（2026-07-30，M0 已完成：source + install/uninstall + 全局 Claude symlink 闭环）

### 1.1 命令表面（M0 已实现 source/install/uninstall，M1+ 补 profile/project/serve）

已实现（M0，9 commit `02e71fe`→`b319d1d`）：
```
skillkit source add|list|remove                            # ✅ 安装源管理（含 --skills-dir / --ref）
skillkit install add <src> <skill> [--scope global|local]  # ✅ 装到 canonical + global 自动建 Claude symlink
skillkit uninstall <id>                                    # ✅ 清 canonical + registry
```
待实现（M1-M2，spec §11）：
```
skillkit install list|upgrade            # M1
skillkit profile create|add-skill|...    # M1（粗分类候选集）
skillkit project add|scan|apply-profile|apply|status  # M1（★ apply 幂等落地是核心）
skillkit serve [--port]                  # M2（本地 web GUI）
```
全部命令支持 `--json`（M1+ 逐步），供 AI agent 操作。

**M0 命令表面偏差**：install 用 `install add <source> <skill>`（两参数），spec §11 是 `install <id>`（单 id）。M1 对齐（待定：解析 id 或保留 add 子命令）。

### 1.2 结构性事实（不变量 + 当前进度）

- 项目位置：`/Users/mywo/lab/skillkit`，**已 git init**（提交历史见 `git log --oneline`，与 mac-config 平级的独立项目）。
- 技术栈：Rust + Axum（M2），单二进制，core + cli + server 三层共享 core，前端 React+Vite 产物 rust-embed 嵌入。无常驻 daemon。
- **M0 已完成**：core 8 模块（paths/error/config/source/registry/git/install/symlink）+ CLI source/install/uninstall + e2e。**17 tests 全绿**（core 单元 14 + e2e 3），clippy `pedantic -D warnings` 零 warning。
- **skills_dir 新增**（本轮）：Source 加 `skills_dir: Option<String>` 字段，支持一个 git/local 源仓库含多个 skill（在 `skills/` 子目录下）。install clone 到临时、取 `<skills_dir>/<skill>` 平铺到 canonical。spec §8.2 已同步更新。真实仓库验证：datacenter-skills/logseq 通过。
- **工程化已就绪**：Cargo workspace（`crates/core` + `crates/cli`）、`rustfmt.toml`（行宽 100）、clippy `pedantic` + `-D warnings`（`[workspace.lints]`）、`Makefile`（setup/format/lint/test/build/check）、`make check` 全绿。
- **设计定稿**：spec（review 完成 P0/P1/P2）+ 决策纪要（12 条）+ `CLAUDE.md`（项目规范）。
- **GUI demo 定稿**：`demo/index.html`（亮色原型，四大视图 + Projects apply 闭环可交互）。
- **M0 计划**：`docs/superpowers/plans/2026-07-29-skillkit-m0.md`（8 个 TDD task，已执行完）。
- 存储约束（不变）：`~/.agents/skills/` 只放全局公共 skill、元数据统一 `~/.skm/`、单版本、shared 只读、id 引用 DRY、agent 能力配置驱动。
- 落地（P0 决策）：local skill **平铺**在 `<agent>/skills/<skill>/`（Claude 只发现一层），git 忽略用 `<project>/.git/info/exclude`（不改项目 `.gitignore`）。

### 1.3 install / build / run flow

```bash
cd /Users/mywo/lab/skillkit
make check                    # format && lint && test（17 tests 全绿）
make build                    # cargo build --all
cargo run -p skillkit-cli -- --help
# 真实 install 验证（用 HOME=tempdir 不污染 ~/.agents/skills/）：
TESTHOME=$(mktemp -d)
HOME=$TESTHOME cargo run -p skillkit-cli -- source add dc git <url> --skills-dir skills
HOME=$TESTHOME cargo run -p skillkit-cli -- install add dc <skill> --scope global
```

## 2. 本会话累积的改动（按时间倒序）

6. **M0 红绿实现**（本轮，9 commit `02e71fe`→`b319d1d`）
   - 方式：主人定推倒重做 + 严格 TDD 红绿 + 每 task 一 commit。先 stash 备份 inline 版，从 `ed1813b` 起逐 task 红绿（inline 备份已丢弃，红绿版更优）。
   - 红绿纪律：每核心 task 先写失败测试 → `cargo test` 确认编译失败（方法/函数未定义，证明测试非恒真）→ 补实现 → `make check` 全绿 → commit。CLI 薄壳 + e2e 验收测试不适用红绿（无单测/测已实现 API），靠 clippy + 手动流程 + 真实仓库验证。
   - commit 序列：`02e71fe` paths+error → `b5e3b96` config → `d8a1765` docs(spec skills_dir) → `c240f45` source+CLI → `e4d637e` registry → `8eefe6f` git+install(core) → `d6b657c` install CLI → `57a4ac6` symlink → `b319d1d` e2e。
   - skills_dir 设计（主人两个私有仓库案例驱动）：Source 加 skills_dir，install clone 临时取子目录平铺，datacenter-skills/logseq 真实验证通过。
   - 计划修正（红绿中抓到）：`SourceType` 加 `Copy`、测试 `reloaded` 加 `mut`、`bare_repo` 带 `-c user`、symlink `existing.as_path()==target`、e2e 放 `crates/core/tests/`（纯 workspace 根 `tests/` 不识别）、lib re-export `Scope`（Task 5 漏，e2e/symlink 两次踩到根治）。
   - 验证：17 tests + clippy 零 warning + 真实仓库 install/uninstall 闭环。

5. **Rust playbook 集成到 project-initialization skill**（全局 skill，不在 skillkit repo）
   - 新建 `playbooks/rust.md` + SKILL.md 登记 + recon 识别 Cargo.toml。skillkit 工程化是首个 Rust 验证案例。

4. **Rust 工程化初始化**（commit `df2f3d6`）
   - Cargo workspace + rustfmt（行宽 100）+ clippy pedantic -D warnings（[workspace.lints]）+ Makefile。

3. **M0 实现计划**（commit `722eb6a`）：8 个 TDD task。

2. **GUI demo**（commit `f6909c7`）：单文件 HTML 亮色原型。

1. **spec review + CLAUDE.md**（commit `cdb8626`）：P0/P1/P2 落实。

0. **初始设计会话**（commit `4a4b78f`）：spec + 决策纪要（11 条）。

## 3. 关键背景知识

### 3.1 `~/.agents/skills/` 是通用加载目录（最重要的约束）
- 除 Claude Code 外，Cursor、OpenCode、Codex、Gemini CLI 等大部分 agent 都直接从 `~/.agents/skills/` 加载 skill。
- 该目录只能放全局公共 skill，不能挪用为项目级暂存，不能放元数据文件（会被 agent 误扫描）。元数据统一收 `~/.skm/`。
- 全局公共 canonical 选在这里让 Cursor 等零配置可用，只有 Claude 需额外 symlink 桥接。

### 3.2 Cursor 不支持 symlink
- Cursor 无法识别 symlink 形式的 skill，必须用真实文件。项目 local skill 对 Cursor 用 copy 兜底（M1 apply）。全局层面 Cursor 直读 `~/.agents/skills/`。

### 3.3 npx skills 的能力与限制
- 能力：从 skills.sh 源下载到 `~/.agents/skills/`，支持私有 git 仓库。限制：安装路径写死，无法指定目录/profile 隔离。这是 skillkit 自研核心的根本原因。M0 的 skills-sh 源用 git clone 占位，正式经 npx skills 是后续。

### 3.4 主人现有 skill 分布（迁移基础，M3 用，迁移时用 `ls | wc -l` 重新统计）
- `~/.agents/skills/`：约 64 个（npx skills 管理）。`~/.claude/skills/`：约 26 真实目录 + 7 symlink。`~/.codex/skills/`：约 10 个。`~/.cursor/skills` 与 `skills-cursor` 并存（疑似试验残留，M3 留意）。`~/.claude/plugins/`：4 个（skillkit 不碰）。

### 3.5 命名排查：skillkit（brew + crates.io 双干净，已选定）。

### 3.6 local skill 平铺落地（P0 决策，最关键的实现约束）
- Claude Code 只发现 `.claude/skills/<skill>/SKILL.md` 一层，子目录完全不发现（issue #39138）；不支持自定义 skill 路径（issue #22902 未实现）。
- local 与 shared 同级平铺在 `<agent>/skills/<skill>/`；区分靠 skillkit 的落地清单（installed_skills 里 scope=local）。
- git 忽略用 `<project>/.git/info/exclude`（git 天然本地、不入库），不改项目 `.gitignore`，apply 动态维护。决策 12 记录方案 + 三个被否备选。

### 3.7 `locked_shas` 是变更基线，非版本锁
- 单版本模型下 canonical 物理只有一份，locked_shas 锁不住版本。它是上次 apply 的 commit_sha 快照，用于检测 canonical 升级漂移。

### 3.8 跨进程 SSE + 文件锁（M2）
- CLI 与 server 是两个进程，server 用 notify file watcher 监听 `~/.skm/` 状态变化经 SSE 推送。文件锁粒度到单文件（registry 一把、每个 project 各一把），读不抢锁，写锁带超时。

### 3.9 skills_dir：一仓库多 skill（M0 本轮新增）
- spec §8.2 原假设 git 源仓库根 = skill 内容。主人真实私有仓库（datacenter-skills、work-agent）是 `skills/` 子目录含多 skill（bb-review/logseq/zai 等）。
- Source 加 `skills_dir: Option<String>`（None=skill 在仓库根，Some("skills")=skill 在该子目录下）。install clone 到临时（`std::env::temp_dir()`，core 生产代码不依赖 tempfile dev-dep）取 `<skills_dir>/<skill>` 平铺到 canonical，保持 Claude 单层发现。
- canonical 绝不残留中间层（`skills/` 不进 canonical），否则 Claude 发现不了 SKILL.md。
- CLI `source add --skills-dir`（clap 把字段 `skills_dir` 的 long 规范化为 `--skills-dir` 连字符，不是下划线）。真实验证：datacenter-skills/logseq。

## 4. 验证清单（重载 / 切换后立即跑）

- [ ] `git -C /Users/mywo/lab/skillkit log --oneline -12` 看到 M0 红绿 9 commit（最新 `b319d1d` e2e）。
- [ ] `cd /Users/mywo/lab/skillkit && make check` 全绿（17 tests）。
- [ ] `cargo run -p skillkit-cli -- --help` 显示 source/install/uninstall。
- [ ] `ls crates/core/src/` 看到 8 模块；`ls crates/core/tests/` 看到 m0_e2e.rs。
- [ ] `crates/*/Cargo.toml` 都有 `[lints] workspace = true`。
- [ ] spec §6.3 写的是 local 平铺 + `.git/info/exclude`（不是 `local/` 子目录）；§8.2 有 skills_dir 字段。
- [ ] **回归信号**：活跃设计里搜不到 `skills/local` 路径（仅决策 12 背景保留被否方案）。
- [ ] **回归信号**：`cargo clippy --all-targets -- -D warnings` 零 warning。
- [ ] **回归信号**：spec 里出现 `.skm/shared.lock` 说明用了旧设计，应删除。
- [ ] **skills_dir 平铺**：install 后 `canonical/<skill>/SKILL.md` 直接在 skill 下，无 `skills/` 中间层。

## 5. 已知遗留 / 待办

1. **now_iso() 固定时间戳占位**（`install.rs`）：生产 installed_at 全是 `2026-07-29T00:00:00Z`，失真。M1 接真实时间（chrono 或 std::time）。
2. **install 命令表面对齐 spec**：`install add <src> <skill>` vs spec `install <id>`。M1 定。
3. **M1 实现（阻塞项，下一个里程碑）**：profile / project / apply 闭环（spec §9-§11）。apply 幂等落地是核心。`--json` schema 锁定测试。
4. **M2**：GUI server（Axum + rust-embed + SSE 跨进程推送）。
5. **M3**：现有 skill 迁移（`~/.agents/` `~/.claude/` `~/.codex/` `~/.cursor/`，能溯源标记版本，无法溯源标记 unmanaged）。
6. ~~M0 Task 2-8~~ ✅ 完成（红绿 + 9 commit）。
7. ~~spec 待 review / M0 计划 / git init / CLAUDE.md~~ ✅ 均完成。

## 6. 关键文件路径速查

```
/Users/mywo/lab/skillkit/                   # 项目根（已 git init）
├── CLAUDE.md                               # 项目规范（代码层硬约束）
├── Cargo.toml                              # workspace 根 + [workspace.lints.clippy]
├── Cargo.lock                              # 提交（bin workspace 锁依赖）
├── Makefile / rustfmt.toml                 # 统一入口 + 格式
├── crates/
│   ├── core/                               # skillkit-core（lib）— 全部业务逻辑（M0 完成）
│   │   ├── Cargo.toml                      # [lints] workspace = true
│   │   ├── src/
│   │   │   ├── lib.rs                      # crate 入口，re-export 子模块
│   │   │   ├── paths.rs                    # Paths 路径解析（生产/测试注入）
│   │   │   ├── error.rs                    # SkillkitError + Result + atomic_write
│   │   │   ├── config.rs                   # Config + Agent 能力（config.toml）
│   │   │   ├── source.rs                   # SourceType/Source(含 skills_dir)/SourcesStore
│   │   │   ├── registry.rs                 # Scope/SkillMeta/Registry（registry.json）
│   │   │   ├── git.rs                      # git 操作封装（clone/rev-parse，系统 git）
│   │   │   ├── install.rs                  # install/uninstall（含 skills_dir 平铺）
│   │   │   └── symlink.rs                  # 全局 Claude symlink 桥接（幂等）
│   │   └── tests/m0_e2e.rs                 # M0 端到端（install→skills_dir→symlink→幂等）
│   └── cli/                                # skillkit-cli（bin: skillkit）— 薄壳
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                     # clap 入口（Source/Install/Uninstall）
│           └── commands/{mod,source,install}.rs
├── demo/index.html                         # GUI 亮色原型（apply 闭环可交互）
└── docs/
    ├── 2026-07-29-skillkit-design.md       # spec（review 完成，§8.2 加了 skills_dir）
    ├── design-decisions-2026-07-29.md      # 决策纪要（12 条）
    ├── superpowers/{plans,specs}/          # M0 计划 + demo 设计文档
    └── sessions/2026-07-29-skillkit-design.md  # 本交接材料
```

## 7. 下次接续工作的最短路径（M1 实现阶段）

### 7.1 冷启动（新会话第一件事）

```bash
cd /Users/mywo/lab/skillkit
git log --oneline -12                    # 确认 M0 红绿 9 commit，最新 b319d1d
make check                               # 17 tests 全绿
```

验证：新会话能复述 skills_dir 平铺（§3.9）、install add 命令表面偏差（§1.1）、now_iso 占位（§5.1）、M1 从 profile/project 起。

### 7.2 当前焦点：M1 profile/project/apply 闭环

spec §9（profile）§10（project）§11（命令）。用 `superpowers:writing-plans` 做 M1 计划 → TDD 红绿执行（沿用 M0 的红绿 + 每 task commit 节奏）：
- **profile**：粗分类候选集（`~/.skm/profiles/<name>.toml`，存 skill id 列表，DRY）。
- **project**：精确管理（`~/.skm/projects/<name>.toml`，`installed_skills` + `locked_shas`）。`apply-profile` 批量入候选，`add-skill`/`remove-skill` 精确调整。
- **apply（★ 核心）**：幂等落地。scope=global 只检查 canonical + Claude symlink 在位（不 per-project 落地）；scope=local 按 §10.2 在 `<project>/<agent>/skills/` 落地（Claude symlink / Cursor copy）+ `.git/info/exclude`。
- `--json` schema 锁定测试（AI agent 依赖稳定）。
- 顺手修 §5.1 now_iso（接真实时间）+ §5.2 install 命令表面对齐。

### 7.3 焦点优先级
1. M1（profile/project/apply 闭环）→ 2. M2（GUI server）→ 3. M3（迁移打磨）。

## 7.x (archive) 历史接续路径

- M0 阶段（已完成）：工程化 → M0 计划 → 红绿实现 8 task。
- 设计阶段（已完成）：写 CLAUDE.md → review spec（P0/P1/P2）→ writing-plans M0 计划。
