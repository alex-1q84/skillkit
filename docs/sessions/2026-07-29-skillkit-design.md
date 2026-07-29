# 2026-07-29 skillkit（设计 → review → 工程化 → M0 启动）

> 用途：把 skillkit 工具会话的关键事实、决策、遗留集中沉淀，便于下次在 skillkit 目录启动新会话时接续。
>
> **新会话最快入口**：直接读 §1（当前状态）+ §4（验证清单）+ §7（下次接续路径），三段够用；细节再回查 §2/§3/§5/§6。

## 1. 当前状态（2026-07-29，设计定稿 + 工程化就绪 + M0 待实现）

### 1.1 命令表面（M0 骨架已能跑，具体逻辑 Task 2+ 实现）

`skillkit` binary 已建（clap 骨架），`source`/`install`/`uninstall` 子命令占位（打印「M0 待实现」）。目标命令表面（spec §11，M0-M3 逐步实现）：

```
skillkit source add|list|remove              # 安装源管理
skillkit install <id> [--scope global|local] # skill 安装到 canonical
skillkit uninstall|upgrade|list              # skill 维护
skillkit profile create|add-skill|...        # profile（粗分类候选集）
skillkit project add|scan|apply-profile|...  # project（精确管理）
skillkit project apply <project>             # ★ 幂等落地（核心）
skillkit project status <project> [--json]   # diff 输出，AI agent 感知接口
skillkit serve [--port]                      # 本地 web GUI（M2）
```

全部命令支持 `--json`（M1+ 逐步），供 AI agent 操作。

### 1.2 结构性事实（不变量 + 当前进度）

- 项目位置：`/Users/mywo/lab/skillkit`，**已 git init**（提交历史见 `git log --oneline`，与 mac-config 平级的独立项目）。
- 技术栈：Rust + Axum（M2），单二进制，core + cli + server 三层共享 core，前端 React+Vite 产物 rust-embed 嵌入。无常驻 daemon。
- **工程化已就绪**：Cargo workspace（`crates/core` + `crates/cli`）、`rustfmt.toml`（行宽 100）、clippy `pedantic` + `-D warnings`（`[workspace.lints]`）、`Makefile`（setup/format/lint/test/build/check）、`make check` 全绿。
- **设计定稿**：spec（review 完成 P0/P1/P2）+ 决策纪要（12 条）+ `CLAUDE.md`（项目规范）。
- **GUI demo 定稿**：`demo/index.html`（亮色原型，四大视图 + Projects apply 闭环可交互）。
- **M0 计划就绪**：`docs/superpowers/plans/2026-07-29-skillkit-m0.md`（8 个 TDD task）。
- 存储约束（不变）：`~/.agents/skills/` 只放全局公共 skill、元数据统一 `~/.skm/`、单版本、shared 只读、id 引用 DRY、agent 能力配置驱动。
- 落地（P0 决策）：local skill **平铺**在 `<agent>/skills/<skill>/`（Claude 只发现一层），git 忽略用 `<project>/.git/info/exclude`（不改项目 `.gitignore`）。

### 1.3 install / build / run flow

```bash
cd /Users/mywo/lab/skillkit
make check                    # format && lint && test（一站式，全绿）
make build                    # 或 cargo build --all
cargo run -p skillkit-cli -- --help   # skillkit binary（M0 骨架）
```

## 2. 本会话累积的改动（按时间倒序）

5. **Rust playbook 集成到 project-initialization skill**（本轮，全局 skill，不在 skillkit repo）
   - 问题：`~/.claude/skills/project-initialization/` 无 Rust playbook，Rust 项目无法自动路由。
   - 决策：新建 `playbooks/rust.md`（对标 java-spring.md）+ `SKILL.md` Index 登记 + `language-selection-template.md` 加 Rust 信号/选项 + `recon.py` 识别 `Cargo.toml` + `draft-plan.py` 加 `rust-tooling` label。
   - 理由：skillkit 的工程化实践是首个 Rust 验证案例，沉淀成可复用 playbook。
   - 验证：skillkit 实测 `recon → detection: confident, languages: ['rust']`、`draft-plan` 渲染 rust-tooling 项、`validate-skill.sh` PASSED。

4. **Rust 工程化初始化**（commit `df2f3d6`）
   - 问题：执行 M0 前需统一 format/lint/test 标准，避免「本地过 CI 不过」。
   - 决策：Cargo workspace + `rustfmt.toml`（行宽 100，对标 Java Spotless）+ clippy `pedantic` `-D warnings`（`[workspace.lints]`，对标 Error Prone/NullAway）+ `Makefile`（apply/check 分离）。用 `project-initialization` skill 的 Java 模板维度映射 Rust。
   - 理由：CI 与本地同规则；pedantic 严但 allow 项目级噪音，不降级跳过。
   - 改动：`Cargo.toml`（workspace + lints）、`crates/core`、`crates/cli`、`rustfmt.toml`、`Makefile`、`.gitignore`（Cargo.lock 策略）、`CLAUDE.md`（§9 make 命令 + §5 format/lint gate）。
   - 验证：`make check` 全绿、clippy pedantic 无 warning。

3. **M0 实现计划**（commit `722eb6a`）
   - 决策：8 个 TDD task（workspace→paths/error→config→source→registry→install→symlink→e2e），每个含写失败测试→实现→通过的 bite-sized step + 完整 Rust 代码。
   - 改动：`docs/superpowers/plans/2026-07-29-skillkit-m0.md`。
   - 验证：计划自审通过（修了 Task 4 add 函数的 placeholder；类型一致）。

2. **GUI demo**（commit `f6909c7`）
   - 问题：文字 spec 难暴露交互层体验问题，需可点击原型做最终设计 review。
   - 决策：单文件 HTML 亮色原型（暖米白工程风，IBM Plex + 琥珀强调），四大视图 + Projects 的 apply 闭环可交互（勾选→APPLY→status diff）。
   - 改动：`demo/index.html`、`docs/superpowers/specs/2026-07-29-skillkit-demo-design.md`。
   - 验证：JS 语法检查通过、Firefox 实测交互、主人 review 定稿。

1. **spec review + CLAUDE.md**（commit `cdb8626`）
   - 问题：spec 自审后需主人 review 确认，且动手前要项目规范。
   - 决策：CLAUDE.md 项目规范；spec review 落实 P0/P1/P2。
     - **P0**：local skill 平铺落地 + `.git/info/exclude`（原子目录方案被 Claude 发现规则否决，issue #39138/#22902）。
     - **P1**：`locked_shas` 语义（变更基线非版本锁）、跨进程 SSE（file watcher）、文件锁粒度（读不锁/写按文件/带超时）。
     - **P2**：各 agent 私有目录迁移、global skill 的 apply 分支、`project scan` 语义。
   - 改动：`CLAUDE.md`（新建）、spec 13 处、决策纪要（决策 6 更新 + 决策 12 新增 + 决策 3/9 补充）。
   - 验证：grep 确认活跃设计无 `skills/local` 残留、步骤连贯、回归信号无新增。

0. **初始设计会话**（commit `4a4b78f`）— 完成完整设计 spec + 决策纪要（11 条）。详见 `docs/2026-07-29-skillkit-design.md` + `docs/design-decisions-2026-07-29.md`。

## 3. 关键背景知识

### 3.1 `~/.agents/skills/` 是通用加载目录（最重要的约束）

- 除 Claude Code 外，Cursor、OpenCode、Codex、Gemini CLI 等大部分 agent 都直接从 `~/.agents/skills/` 加载 skill（project 级则是 `<project>/.agents/skills/`）。
- 因此该目录只能放全局公共 skill，不能挪用为 skillkit 的项目级暂存，也不能在里面放 `.registry.json` 之类的元数据文件（会被 agent 误扫描）。
- 全局公共 canonical 选在这里本身让 Cursor 等零配置可用，只有 Claude 需要额外 symlink 桥接（Claude 不直接读 .agents）。
- 来源：npx skills README 的 supported-agents 表 + 主人实测确认。

### 3.2 Cursor 不支持 symlink

- Cursor 无法识别 symlink 形式的 skill，必须用真实文件。所以项目 local skill 对 Cursor 用 copy 兜底，apply 时按 canonical 内嵌的 commit_sha 检测副本过期、过期重 copy。
- 全局层面 Cursor 直读 `~/.agents/skills/`，所以全局公共 skill 对 Cursor 无需任何操作。

### 3.3 npx skills 的能力与限制

- 能力：从 skills.sh 源下载 skill 到 `~/.agents/skills/`，支持私有 git 仓库（底层就是 git clone）。
- 限制：安装路径写死，无法指定目录、无法做 profile 隔离。这正是 skillkit 要自研核心的根本原因。

### 3.4 主人现有 skill 分布（迁移基础，M3 用，迁移时用 `ls | wc -l` 重新统计）

- `~/.agents/skills/`：约 64 个（npx skills 管理，部分已 symlink 到 claude）。
- `~/.claude/skills/`：约 26 个真实目录（手动放）+ 7 个 symlink → `.agents/`。
- `~/.codex/skills/`：约 10 个。另有 `~/.cursor/skills` 与 `~/.cursor/skills-cursor` 并存（疑似试验残留，M3 留意）。
- `~/.claude/plugins/`：4 个插件（superpowers/skill-creator/knowledge-copilot/cli-anything，由原生系统管，skillkit 不碰）。

### 3.5 命名排查结论

- `skm`/`knack`/`skiff` 均撞名（brew/crates.io）；**`skillkit`**：brew + crates.io 双干净，已选定。

### 3.6 local skill 平铺落地（P0 决策，最关键的实现约束）

- Claude Code 只发现 `.claude/skills/<skill>/SKILL.md` 一层，子目录（含 `local/`）**完全不发现**（[issue #39138](https://github.com/anthropics/claude-code/issues/39138)）。
- Claude Code **不支持自定义 skill 路径**（[issue #22902](https://github.com/anthropics/claude-code/issues/22902) 未实现）。
- 所以 local 与 shared 同级平铺在 `<agent>/skills/<skill>/`；区分靠 skillkit 的落地清单（`installed_skills` 里 scope=local 的）。
- git 忽略用 `<project>/.git/info/exclude`（git 天然本地、不入库，团队成员互不冲突），**不改项目 `.gitignore`**。apply 动态维护。
- 决策 12 记录方案选择 + 三个被否备选（含「取消物理隔离全部装全局池」的方案 B）。

### 3.7 `locked_shas` 是变更基线，非版本锁

- 单版本模型下 canonical 物理只有一份，`locked_shas` 锁不住版本。
- 它是上次 apply 的 commit_sha 快照，用于检测 canonical 升级漂移（apply/upgrade 时比对、提示受影响项目），不是多版本锁。

### 3.8 跨进程 SSE + 文件锁

- CLI 与 server 是两个独立进程，server 用 `notify` file watcher 监听 `~/.skm/` 状态文件变化经 SSE 推送，CLI 不必通知 server。
- 文件锁粒度到单文件（registry 一把、每个 project 各一把），读操作（status/list）不抢锁，写锁带超时，超时按冲突报错而非死等。

## 4. 验证清单（重载 / 切换后立即跑）

- [ ] `git -C /Users/mywo/lab/skillkit log --oneline -6` 看到 5 个 commit（最新 `df2f3d6` 工程化）。
- [ ] `cd /Users/mywo/lab/skillkit && make check` 全绿（format/lint/test）。
- [ ] `cargo run -p skillkit-cli -- --help` 显示 source/install/uninstall 子命令。
- [ ] `ls crates/` 看到 core + cli；`crates/*/Cargo.toml` 都有 `[lints] workspace = true`。
- [ ] `ls docs/superpowers/{plans,specs}/` 看到 M0 计划 + demo 设计文档。
- [ ] spec §6.3 写的是 local 平铺 + `.git/info/exclude`（不是 `local/` 子目录）。
- [ ] **回归信号**：活跃设计里搜不到 `skills/local` 路径（仅决策 12 背景保留被否方案）。
- [ ] **回归信号**：`cargo clippy --all-targets -- -D warnings` 若报 warning，说明 lints 失效或代码退化。
- [ ] **回归信号**：spec 里出现 `.skm/shared.lock` 说明用了旧设计（shared 曾由 skillkit 管），应删除。

## 5. 已知遗留 / 待办

1. **M0 实现（Task 2-8）** — **阻塞项**。workspace 骨架（Task 1）已由工程化完成，从 Task 2（paths + error）起。用 `superpowers:subagent-driven-development`（推荐）或 `executing-plans` 执行 `docs/superpowers/plans/2026-07-29-skillkit-m0.md`。
2. ~~spec 待主人最终 review~~ ✅ 已完成（P0/P1/P2 落实）。
3. ~~M0 实现计划未做~~ ✅ 已完成（8 task）。
4. ~~skillkit 未 git init~~ ✅ 已 init。
5. ~~skillkit/CLAUDE.md 未写~~ ✅ 已完成。
6. **现有 skill 迁移（M3）** — 扫描导入 `~/.agents/`、`~/.claude/`、`~/.codex/`、`~/.cursor/` 等（能确定源的标记版本，无法溯源的标记 unmanaged）。优先级低。
7. **Rust playbook 反哺**：`~/.claude/skills/project-initialization/playbooks/rust.md` 已集成（全局），skillkit 是首个验证案例；后续 Rust 项目可直接复用。

## 6. 关键文件路径速查

```
/Users/mywo/lab/skillkit/                   # 项目根（已 git init）
├── CLAUDE.md                               # user-visible — 项目规范（代码层硬约束）
├── Cargo.toml                              # workspace 根 + [workspace.lints.clippy]
├── Cargo.lock                              # 提交（bin workspace 锁定依赖）
├── Makefile                                # setup/format/lint/test/build/check
├── rustfmt.toml                            # 行宽 100，stable 选项
├── crates/
│   ├── core/                               # skillkit-core（lib）— 业务逻辑（M0 Task 2+ 填充）
│   │   ├── Cargo.toml                      # [lints] workspace = true
│   │   └── src/lib.rs                      # 仅文档注释（Task 2 起加模块）
│   └── cli/                                # skillkit-cli（bin: skillkit）— 薄壳
│       └── src/main.rs                     # clap 骨架（source/install/uninstall 占位）
├── demo/
│   └── index.html                          # GUI 亮色原型（apply 闭环可交互，已定稿）
├── docs/
│   ├── 2026-07-29-skillkit-design.md       # user-visible — spec（review 完成，16 节）
│   ├── design-decisions-2026-07-29.md      # user-visible — 决策纪要（12 条）
│   ├── superpowers/
│   │   ├── plans/2026-07-29-skillkit-m0.md # M0 实现计划（8 task TDD）
│   │   └── specs/2026-07-29-skillkit-demo-design.md  # demo 设计文档
│   └── sessions/
│       └── 2026-07-29-skillkit-design.md   # internal — 本交接材料
```

外部（全局 skill，不在本 repo）：
- `~/.claude/skills/project-initialization/playbooks/rust.md` — 本轮新增 Rust playbook

## 7. 下次接续工作的最短路径（M0 实现阶段）

### 7.1 冷启动（新会话第一件事）

```bash
cd /Users/mywo/lab/skillkit
git log --oneline -6                                        # 确认 5 commit，最新 df2f3d6
make check                                                  # 确认全绿
sed -n '1,80p' docs/superpowers/plans/2026-07-29-skillkit-m0.md  # M0 计划头部
```

验证：新会话能复述 local 平铺落地 + `.git/info/exclude`（P0）、`locked_shas` 是变更基线（P1）、M0 从 Task 2 起。

### 7.2 当前焦点：执行 M0 Task 2-8

M0 Task 1（workspace 骨架）已由工程化完成。从 Task 2（paths + error）起，逐 task TDD：

```
读 docs/superpowers/plans/2026-07-29-skillkit-m0.md 的 Task 2 → 写失败测试 → 跑失败 →
最小实现 → 跑通过 → commit → make check 验证 → 下一 task
```

执行方式（主人定）：
- `superpowers:subagent-driven-development`（推荐）：每 task 一个 fresh subagent + 任务间 review。
- `executing-plans`：本会话 inline 批量 + checkpoint。

### 7.3 焦点优先级

1. M0 Task 2（paths + error）→ 3（config）→ 4（source）→ 5（registry）→ 6（install）→ 7（symlink）→ 8（e2e）。
2. M0 完成后：M1（profile/project/apply 闭环）→ M2（GUI server）→ M3（迁移打磨）。

## 7.x (archive) 第 1 次接续的最短路径（设计阶段，已完成）

1. ~~写 `skillkit/CLAUDE.md`~~ ✅
2. ~~主人 review spec~~ ✅（P0/P1/P2）
3. ~~writing-plans 做 M0 实现计划~~ ✅

冷启动当时是 `ls docs/` 确认 spec + 决策纪要；skillkit 当时未 git init。这些均已推进，见 §1。
