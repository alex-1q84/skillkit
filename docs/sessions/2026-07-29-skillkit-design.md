# 2026-07-31 skillkit（设计 → review → 工程化 → M0 → M1 完成）

> 用途：把 skillkit 工具会话的关键事实、决策、遗留集中沉淀，便于下次在 skillkit 目录启动新会话时接续。
>
> **新会话最快入口**：直接读 §1（当前状态）+ §4（验证清单）+ §7（下次接续路径），三段够用；细节再回查 §2/§3/§5/§6。

## 1. 当前状态（2026-07-31，M1 完成：profile + project + apply 幂等落地闭环，核心 CLI 可用）

### 1.1 命令表面（M0+M1 已实现 source/install/profile/project，M2 补 serve）

已实现（M0+M1）：
```
skillkit source add|list|remove                            # ✅ 安装源管理（含 --skills-dir / --ref）
skillkit install add <src> <skill> [--scope global|local]  # ✅ 装到 canonical + global 自动建 Claude symlink
skillkit uninstall <id>                                    # ✅ 清 canonical + registry
skillkit profile create|add-skill|remove-skill|list        # ✅ 粗分类候选集
skillkit project add|rebind|scan|list                      # ✅ add 生成 uuid project-id；rebind 重绑定（id 不变）
skillkit project apply-profile|add-skill|remove-skill      # ✅ 声明编辑 installed_skills
skillkit project apply [--frozen] [--json]                 # ✅ 幂等落地（Claude symlink / Cursor copy）
skillkit project status [--json]                           # ✅ {expected, missing, extra, conflicts} id 清单
```
待实现（M2-M3，spec §11/§12）：
```
skillkit install list|upgrade            # M3
skillkit serve [--port]                  # M2（本地 web GUI）
skillkit import-existing                 # M3（扫描导入现有 skill）
```
全部命令支持 `--json`（M1 起 status/apply 已支持），供 AI agent 操作。

**命令表面备注**：install 用 `add <source> <skill>` 子命令（spec §11 `install <id>` 的显式拆分形式，已注明）；project-id 用 uuid 短码（非 path sanitize）。

### 1.2 结构性事实（不变量 + 当前进度）

- 项目位置：`/Users/mywo/lab/skillkit`，**已 git init**（与 mac-config 平级的独立项目）。
- 技术栈：Rust + Axum（M2），单二进制，core + cli + server 三层共享 core，前端 React+Vite 产物 rust-embed 嵌入。无常驻 daemon。
- **M0+M1 完成**：core 11 模块（paths/error/config/source/registry/git/install/symlink + **profile/project/apply**）+ CLI source/install/profile/project + e2e。**35 tests 全绿**（core 单元 29 + m0_e2e 3 + m1_e2e 3），clippy `pedantic -D warnings` 零 warning。
- **apply 闭环（M1 核心，spec §10）**：`compute_diff`（expected/conflicts）→ `land_one`（按 agent 能力 symlink/copy + `.skillkit-sha` 标记）→ `run_apply`（global ensure + local 落地 + extra 清理 + locked_shas 更新 + `--frozen`）→ `build_status`（id 清单）。Cursor copy 过期（sha 漂移）自动重 copy；`.git/info/exclude` 维护 skillkit 段。
- **project-id（M1）**：uuid v4 前 8 hex 短码，独立于 path；`project rebind <id> <new-path>` 支持项目移动/改名（id 不变、path/name 更新）。
- **skills_dir（M0）**：Source 加 `skills_dir` 字段，支持一个 git/local 源仓库含多个 skill（在 `skills/` 子目录下）。install clone 到临时、取 `<skills_dir>/<skill>` 平铺到 canonical。真实仓库验证：datacenter-skills/logseq 通过。
- **工程化已就绪**：Cargo workspace（`crates/core` + `crates/cli`）、`rustfmt.toml`（行宽 100）、clippy `pedantic` + `-D warnings`（`[workspace.lints]`）、`Makefile`（setup/format/lint/test/build/check/**run**）、`make check` 全绿。
- **设计定稿**：spec（review 完成 P0/P1/P2，§8.2 加 skills_dir）+ 决策纪要（12 条）+ `CLAUDE.md`（项目规范，含 M0 踩坑约定：re-export 完整/集成测试位置/git identity/make run）。
- **GUI demo 定稿**：`demo/index.html`（亮色原型，四大视图 + Projects apply 闭环可交互，M2 产品化）。
- 存储约束（不变）：`~/.agents/skills/` 只放全局公共 skill、元数据统一 `~/.skm/`、单版本、shared 只读、id 引用 DRY、agent 能力配置驱动。
- 落地（P0 决策）：local skill **平铺**在 `<agent>/skills/<skill>/`（Claude 只发现一层），git 忽略用 `<project>/.git/info/exclude`（不改项目 `.gitignore`）。agent name → 目录映射（claude-code→.claude）。

### 1.3 install / build / run flow

```bash
cd /Users/mywo/lab/skillkit
make check                    # format && lint && test（35 tests 全绿）
make build                    # cargo build --all
make run ARGS="--help"        # 跑最新 CLI（避免 make check 后旧 bin）
# apply 真实验证（HOME=tempdir 不污染 ~/.agents/skills；cargo 依赖真实 HOME 找 toolchain，
# 所以先 make build 产出 bin，再用 HOME=tempdir 跑 bin）：
make build
TESTHOME=$(mktemp -d)
HOME=$TESTHOME ./target/debug/skillkit source add local local <path> --skills-dir skills
HOME=$TESTHOME ./target/debug/skillkit install add local <skill> --scope local
HOME=$TESTHOME ./target/debug/skillkit project add <demo>
HOME=$TESTHOME ./target/debug/skillkit project apply <id>
```

## 2. 本会话累积的改动（按时间倒序）

7. **M1 实现**（本轮，9 commit `8e1b5a6`→`78fd358` + 计划 `57ce29d` + 修正 `84ab57c`）
   - 方式：主人定 inline + 严格 TDD 红绿 + 每 task commit。先 writing-plans 做 M1 计划（10 task），review 后执行。
   - 红绿纪律：每核心 task 先写失败测试 → 看红 → 补实现 → 看绿 → commit。CLI 薄壳 + e2e 靠 clippy + 手动 + 真实仓库验证。
   - commit：profile(core/cli) → project(core + uuid id + rebind / cli) → 声明编辑 → apply diff → apply 落地+编排（Task 7+8 合并，run_apply 调 land_one 消除 dead_code）→ status/apply CLI → e2e。
   - 红绿抓到并修的计划 bug：**agent_dir 映射**（agent name "claude-code" → 目录 ".claude"，非 ".claude-code"）、**extra 清理路径**（用 agent_dir_name，非 agent name）、**StatusView 计数→id 清单**（spec §11 agent 决策）、**ApplyDiff 去冗余 missing/extra**、多处 clippy（map_or→is_some_and、manual_let_else、inefficient_to_string→`(*s).to_owned()`、default_trait_access）。
   - 设计决策（spec 未明，主人确认）：project-id = uuid v4 前 8 hex + rebind；Cursor copy 用 `.skillkit-sha` 标记；install add 保留；status --json 给 id 清单。
   - 验证：35 tests + clippy 零 warning + 真实 apply 闭环（install local → project apply → .claude/skills symlink → status --json）。

6. **M0 红绿实现 + 项目设置补全**（上一会话，详见 git log）。
5-0. Rust playbook / 工程化 / M0 计划 / GUI demo / spec review / 初始设计（同前）。

## 3. 关键背景知识

### 3.1 `~/.agents/skills/` 是通用加载目录（最重要的约束）
- 除 Claude Code 外，Cursor/OpenCode/Codex/Gemini CLI 等大部分 agent 都直接从 `~/.agents/skills/` 加载。
- 该目录只放全局公共 skill，不能挪用为暂存，不能放元数据文件（会被 agent 误扫描）。元数据统一收 `~/.skm/`。

### 3.2 Cursor 不支持 symlink
- Cursor 无法识别 symlink skill，必须用真实文件。项目 local skill 对 Cursor 用 copy 兜底（`.skillkit-sha` 标记 sha，apply 比对过期重 copy）。全局层面 Cursor 直读 `~/.agents/skills/`。

### 3.3 npx skills 的能力与限制
- 能力：从 skills.sh 源下载到 `~/.agents/skills/`。限制：安装路径写死，无法 profile 隔离。这是 skillkit 自研核心的根本原因。skills-sh 源 M0 用 git clone 占位。

### 3.4 主人现有 skill 分布（迁移基础，M3 用）
- `~/.agents/skills/`：约 64 个。`~/.claude/skills/`：约 26 真实 + 7 symlink。`~/.codex/skills/`：约 10。`~/.cursor/skills` 与 `skills-cursor` 并存（疑似残留）。`~/.claude/plugins/`：4 个（skillkit 不碰）。

### 3.5 命名：skillkit（brew + crates.io 双干净，已选定）。

### 3.6 local skill 平铺落地（P0 决策）
- Claude Code 只发现 `.claude/skills/<skill>/SKILL.md` 一层，子目录不发现（issue #39138）；不支持自定义 skill 路径（issue #22902）。local 与 shared 同级平铺；git 忽略用 `<project>/.git/info/exclude`，不改 `.gitignore`。

### 3.7 `locked_shas` 是变更基线，非版本锁
- 单版本模型下 canonical 物理只有一份，locked_shas 锁不住版本。它是上次 apply 的 sha 快照，用于检测 canonical 升级漂移。apply 时发现漂移默认以 canonical 为准更新基线，`--frozen` 报错。

### 3.8 跨进程 SSE + 文件锁（M2）
- CLI 与 server 两进程，server 用 notify file watcher 监听 `~/.skm/` 经 SSE 推送。文件锁粒度到单文件，读不抢锁，写锁带超时。

### 3.9 skills_dir：一仓库多 skill（M0）
- Source 加 `skills_dir: Option<String>`（None=skill 在仓库根，Some("skills")=在子目录）。install clone 到临时（`std::env::temp_dir()`，core 生产代码不依赖 tempfile dev-dep）取 `<skills_dir>/<skill>` 平铺到 canonical。CLI `source add --skills-dir`（clap 下划线→连字符 long）。真实验证：datacenter-skills/logseq。

### 3.10 apply 落地规则（M1 核心，最关键的实现约束）
- **agent_dir 映射**：agent name "claude-code" → 项目目录 `.claude`（非 `.claude-code`）；其他 agent（cursor 等）name 直接作目录。`land_one`/`write_exclude`/`scan_local_landed`/`build_status` 统一用 `agent_dir_name(agent)`。**踩坑**：忘记映射会建错目录（.claude-code）导致 Claude 发现不了 + extra 清理删错路径。
- **local 落地**：Claude（supports_symlink=true）建 symlink `~/.skm/skills/<skill>` → `<project>/.claude/skills/<skill>`；Cursor（false）copy + 写 `.skillkit-sha`（apply 比对，过期重 copy）。
- **global 不 per-project 落地**：只 ensure `~/.claude/skills/<skill>` symlink 在位（复用 `ensure_global_claude`）。
- **extra 清理**：`scan_local_landed` 找 skillkit-local（symlink 指向 `~/.skm/skills/`，或目录含 `.skillkit-sha`），不在 expected 的删（用 agent_dir_name 拼路径）。
- **`.git/info/exclude`**：skillkit 段（`# >>> skillkit managed >>>` / `<<<` 标记），原子写，列当前 local 落地清单。
- **shared 同名 / local 已被 git 追踪**：land_one 遇真实目录（symlink 模式）报错警告；已追踪提示 `git rm --cached`（部分覆盖，M3 打磨）。

### 3.11 project-id + rebind（M1）
- project-id = `uuid::Uuid::new_v4()` 前 8 hex 大写（如 `A1B2C3D4`），**注册时随机生成、独立于 path**，文件名 `<id>.toml`。
- `Project { id（冻结）, name/path（可变）, agents, applied_profiles, installed_skills, locked_shas }`。
- `project rebind <id> <new-path>`：更新 path/name，id 不变（支持项目移动/改名）。

## 4. 验证清单（重载 / 切换后立即跑）

- [ ] `git -C /Users/mywo/lab/skillkit log --oneline -15` 看到 M1 9 commit（最新 `78fd358` m1_e2e）。
- [ ] `cd /Users/mywo/lab/skillkit && make check` 全绿（35 tests）。
- [ ] `make run ARGS="--help"` 显示 source/install/profile/project。
- [ ] `ls crates/core/src/` 看到 11 模块；`ls crates/core/tests/` 看到 m0_e2e.rs + m1_e2e.rs。
- [ ] `crates/*/Cargo.toml` 都有 `[lints] workspace = true`；core 依赖含 uuid + chrono。
- [ ] spec §8.2 有 skills_dir、§10 apply 流程、§15.2 M1 验收。
- [ ] **回归信号**：活跃设计搜不到 `skills/local`；`cargo clippy --all-targets -- -D warnings` 零 warning；无 `.skm/shared.lock`。
- [ ] **apply 闭环**：install local → project apply → `<project>/.claude/skills/<skill>` symlink；status --json 输出 `{expected, missing, extra, conflicts}`（Vec<String>）。

## 5. 已知遗留 / 待办

1. **M2 GUI server（下一里程碑）**：`skillkit serve` + Axum REST API + SSE（apply 进度推送）+ rust-embed（前端产物嵌入）+ React+Vite（`demo/index.html` 产品化）。spec §12。
2. **M3 迁移打磨**：`import-existing` 扫描导入现有 skill；install upgrade/list；Cursor copy 一致性完善；打包 mac-config Brewfile。
3. **基建债（M0 review 提的中优先级，仍未还）**：CI（GitHub Actions 跑 make check）、README、Cargo.toml `[package]` 元数据（description/license/repository/rust-version）。
4. **local 已被 git 追踪** 提示 `git rm --cached` 仅部分覆盖（land_one 报错，没显式引导命令），M3 打磨。
5. ~~M1 profile/project/apply~~ ✅ 完成（红绿 + 9 commit）。~~M0~~ ✅。

## 6. 关键文件路径速查

```
/Users/mywo/lab/skillkit/
├── CLAUDE.md                               # 项目规范（含 M0 踩坑约定）
├── Cargo.toml / Cargo.lock                 # workspace 根 + [workspace.lints.clippy]
├── Makefile / rustfmt.toml                 # 统一入口（含 run）+ 格式
├── crates/
│   ├── core/                               # skillkit-core（lib）— 全部业务逻辑（M0+M1 完成）
│   │   ├── Cargo.toml                      # uuid + chrono（M1 加）
│   │   ├── src/
│   │   │   ├── lib.rs                      # crate 入口，re-export 子模块
│   │   │   ├── paths.rs                    # Paths（home/skm/agents/claude/skills/projects/profiles）
│   │   │   ├── error.rs                    # SkillkitError + Result + atomic_write
│   │   │   ├── config.rs                   # Config + Agent 能力 + find_agent
│   │   │   ├── source.rs                   # SourceType/Source(含 skills_dir)/SourcesStore
│   │   │   ├── registry.rs                 # Scope/SkillMeta/Registry
│   │   │   ├── git.rs                      # git 操作封装（clone/rev-parse）
│   │   │   ├── install.rs                  # install/uninstall（含 skills_dir 平铺，now_iso 用 chrono）
│   │   │   ├── symlink.rs                  # 全局 Claude symlink 桥接
│   │   │   ├── profile.rs                  # Profile/ProfileStore（M1）
│   │   │   ├── project.rs                  # Project/new_id/register/rebind + list_ids（M1）
│   │   │   └── apply.rs                    # compute_diff/land_one/run_apply/build_status（M1 核心）
│   │   └── tests/{m0_e2e,m1_e2e}.rs        # 端到端
│   └── cli/                                # skillkit-cli（bin: skillkit）— 薄壳
│       ├── Cargo.toml                      # serde_json（M1 加，--json 输出）
│       └── src/{main.rs, commands/{source,install,profile,project}.rs}
├── demo/index.html                         # GUI 亮色原型（M2 产品化）
└── docs/
    ├── 2026-07-29-skillkit-design.md       # spec（§8.2 skills_dir，§10 apply）
    ├── design-decisions-2026-07-29.md      # 决策纪要（12 条）
    ├── superpowers/plans/2026-07-30-skillkit-m1.md  # M1 计划（已执行完，含执行修正）
    └── sessions/2026-07-29-skillkit-design.md      # 本交接材料
```

## 7. 下次接续工作的最短路径（M2 GUI 阶段）

### 7.1 冷启动（新会话第一件事）

```bash
cd /Users/mywo/lab/skillkit
git log --oneline -15                   # 确认 M1 9 commit，最新 78fd358
make check                              # 35 tests 全绿
make run ARGS="project --help"          # 确认 project 子命令
```

验证：新会话能复述 apply 闭环（§3.10：compute_diff→land_one→run_apply→build_status）、agent_dir 映射（claude-code→.claude）、project-id uuid + rebind（§3.11）、skills_dir（§3.9）。

### 7.2 当前焦点：M2 GUI server

spec §12。M2 创建 `crates/server` + `web/`（React+Vite），三个新东西：
- **`skillkit serve`**：Axum web server，localhost 绑定 + 随机 token 防误访问。
- **REST API + SSE**：四大视图（Sources/Skills/Profiles/Projects）的数据端点 + SSE 推送（apply 进度、status 变化，复用 §3.8 notify file watcher）。
- **rust-embed**：前端构建产物嵌入二进制（单二进制，零运行时依赖）。
- 前端把 `demo/index.html`（M0 定稿原型）产品化为 React+Vite，调 Axum API。

用 `superpowers:writing-plans` 做 M2 计划（前后端拆分，先 server + API 再前端）→ TDD 红绿执行（沿用红绿 + 每 task commit 节奏）。

### 7.3 焦点优先级
1. M2（GUI server）→ 2. M3（迁移打磨 + 打包）→ 3. 基建债（CI/README/元数据）。

## 7.x (archive) 历史接续路径

- M1 阶段（已完成）：writing-plans 计划 → 红绿 10 task（profile/project/apply 闭环）。
- M0 阶段（已完成）：工程化 → 红绿 8 task（core 骨架 + install + symlink）。
- 设计阶段（已完成）：写 CLAUDE.md → review spec（P0/P1/P2）→ writing-plans M0。
