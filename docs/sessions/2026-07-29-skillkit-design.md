# 2026-07-29 skillkit 设计阶段（第 1 次接续）

> 用途：把 skillkit 工具的初始设计会话的关键事实、决策、遗留问题集中沉淀，便于下次在 skillkit 目录启动新会话时接续。
>
> **新会话最快入口**：直接读 §1（当前状态）+ §4（验证清单）+ §7（下次接续路径），三段够用；细节再回查 §2/§3/§5/§6。

## 1. 当前状态（2026-07-29 初始设计完成后）

### 1.1 设计中的命令表面（尚未实现）

skillkit 是一个全新的 Rust 项目，目前只有设计文档，无任何代码。下列 CLI 命令是 spec 中定义的目标表面，M0 起逐步实现：

```
skillkit source add|list|remove              # 安装源管理
skillkit install <id> [--scope global|local] # skill 安装到 canonical
skillkit uninstall|upgrade|list              # skill 维护
skillkit profile create|add-skill|...        # profile（粗分类候选集）
skillkit project add|scan|apply-profile|...  # project（精确管理）
skillkit project apply <project>             # ★ 幂等落地（核心）
skillkit project status <project> [--json]   # diff 输出，AI agent 感知接口
skillkit serve [--port]                      # 本地 web GUI
```

全部命令支持 `--json`，供 AI agent 操作。

### 1.2 结构性事实（新会话必须知道的不变量）

- 项目位置：`/Users/mywo/lab/skillkit`（与 mac-config 平级的独立项目，尚未 git init）。
- 技术栈：Rust + Axum，单二进制，核心库 + CLI + web server 共享核心，前端 React+Vite 产物用 rust-embed 嵌入。无常驻 daemon。
- 与 npx skills 的边界：npx skills 只负责从 skills.sh 源下载到 `~/.agents/skills/`，其余（版本/profile/project/落地）全部 skillkit 自研。
- 存储关键约束：`~/.agents/skills/` 是通用 AI agent 加载目录（Cursor/OpenCode/Codex/Gemini 直读），只放全局公共 skill，元数据绝不放进去；所有元数据统一收 `~/.skm/`。
- 落地策略按 agent 能力：Claude 用 symlink，Cursor 不支持 symlink 用 copy 兜底。
- skillkit 不管 Claude 原生插件系统（superpowers 等由 installed_plugins.json 管），也不管 `~/.claude/commands/`。
- 项目 shared skill 不由 skillkit 管（项目 git 自己管），skillkit 只读发现。

### 1.3 install / build / run flow

尚未实现（M0 未开始）。当前"运行"就是读设计文档：

```bash
cat /Users/mywo/lab/skillkit/docs/2026-07-29-skillkit-design.md
```

## 2. 本会话累积的改动（按时间倒序）

1. **初始设计会话（2026-07-29）— 完成完整设计 spec**
   - 问题：skill 生态分散（64 个在 ~/.agents/、26 个手动放 ~/.claude/skills/、跨 agent 同步靠手写 just recipe、无 profile 概念），需要统一管理工具。
   - 决策：做 skillkit——独立实现核心引擎（不包装 npx skills），Rust+Axum 单二进制，symlink 池 + 单版本，local/shared 分类，profile 粗分类 + project 精确选择。
   - 理由：详见 `docs/design-decisions-2026-07-29.md`（11 条决策各含"为什么"和被否备选）。关键几条：npx skills 路径写死做不到 profile 隔离所以必须自研核心；Rust 单二进制毫秒启动对 AI agent 高频调用友好；~/.agents/skills 是通用加载目录所以不能挪用为项目暂存；shared 在 git 里已有管理所以 skillkit 不重复管。
   - 改动：新建 `docs/2026-07-29-skillkit-design.md`（spec，21.8K）、`docs/design-decisions-2026-07-29.md`（决策纪要）。
   - 验证：spec 自审通过（无 placeholder、章节自洽、范围聚焦 M0-M3、无歧义；修了 3 处歧义：.gitignore 与"不写配置文件"的表面矛盾、scope 唯一性约束、profile 主要承载 local）。

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
- 限制：安装路径写死（`.claude/skills/` 或 `~/.claude/skills/`），无法指定目录、无法做 profile 隔离。这正是 skillkit 要自研核心的根本原因。

### 3.4 主人现有 skill 分布（迁移基础，M3 用）

- `~/.agents/skills/`：64 个（npx skills 管理，部分已 symlink 到 claude）。
- `~/.claude/skills/`：26 个真实目录（手动放，脱离版本管理）+ 7 个 symlink → `.agents/`。
- `~/.codex/skills/`：10 个。
- `~/.claude/plugins/`：4 个插件（superpowers/skill-creator/knowledge-copilot/cli-anything，由原生系统管，skillkit 不碰）。

### 3.5 命名排查结论

- `skm` 不可用：brew 上是 TimothyYe/skm（SSH key manager，命令冲突）；crates.io 上 pyrex41/skill-manager 已占且功能几乎一样；reorx/skm 也是同类。
- `knack` 不可用：crates.io 已有同类 Agent Skills CLI。
- `skillkit`：brew + crates.io 双干净，已选定。

## 4. 验证清单（重载 / 切换后立即跑）

- [ ] `ls /Users/mywo/lab/skillkit/docs/` 同时看到 `2026-07-29-skillkit-design.md` 和 `design-decisions-2026-07-29.md`。
- [ ] spec 第 5 节三类 skill 职责表里，"项目 shared" 一行写的是"只读发现"（不是全权管理）。
- [ ] spec 第 6.2 节 `~/.agents/skills/` 只列全局公共 canonical，不含任何 `.skm` 元数据文件。
- [ ] **回归信号**：如果在 spec 或纪要里搜到独立的 `skm` 命令（非 "skillkit" 也非出现在撞名排查上下文），说明命名替换没做干净——应全部改为 `skillkit`。
- [ ] **回归信号**：如果 spec 里出现 `.skm/shared.lock`，说明用了旧设计（shared 曾由 skillkit 管），应删除——shared 不由 skillkit 管。

## 5. 已知遗留 / 待办

1. **spec 待主人最终 review** — 设计已自审，但主人尚未明确拍板"通过"，需主人在新会话过目后确认或提调整。
2. **M0 实现计划未做** — review 通过后用 superpowers:writing-plans 生成 M0（骨架：core 库 + CLI 框架 + source 管理 + install/uninstall + 全局 Claude symlink 桥接）的详细实现计划。这是阻塞项。
3. **skillkit 未 git init** — 新项目还没初始化 git，本交接材料的 commit 步骤因此暂缓。
4. **skillkit/CLAUDE.md 未写** — 按主人"约束先行"原则，动手写代码前应先写项目 CLAUDE.md（技术栈约定、目录结构、测试约定、commit 规范）。优先级高。
5. **现有 skill 迁移（M3）** — 扫描导入 ~/.agents/skills/ 和 ~/.claude/skills/，能确定源的标记版本、手动放的标记 unmanaged。优先级低（M3）。

## 6. 关键文件路径速查

```
/Users/mywo/lab/skillkit/                  # 项目根（无 git）
├── docs/
│   ├── 2026-07-29-skillkit-design.md      # user-visible — 设计 spec（最终设计，16 节）
│   ├── design-decisions-2026-07-29.md     # user-visible — 决策纪要（11 条"为什么"+被否方案）
│   └── sessions/
│       └── 2026-07-29-skillkit-design.md  # internal — 本交接材料
```

**对外可见 vs 内部**：`docs/` 根下的 spec 和决策纪要是项目设计文档（对外可见，团队可读）；`docs/sessions/` 是会话交接材料（内部，给接续会话用）。目前无代码、无配置、无 git。

## 7. 下次接续工作的最短路径（第 1 次接续后）

### 7.1 冷启动（新会话第一件事）

```bash
cd /Users/mywo/lab/skillkit
ls docs/                                    # 确认 spec + 决策纪要在
# 让新会话读设计：
#   docs/2026-07-29-skillkit-design.md（spec）
```

验证：新会话能复述三类 skill 职责、`~/.agents/skills/` 的约束、profile vs project 的分工。

### 7.2 待推送 / 待发布

```bash
git -C /Users/mywo/lab/skillkit status      # 当前报错：不是 git 仓库
```

skillkit 尚未 git init。需先 `git init`、写初始 commit（含 docs/），才能谈推送。按主人习惯不自动 git，等主人指示。

### 7.3 工作区当前焦点

1. 写 `skillkit/CLAUDE.md` — 约束先行，动手前先定项目规范（Rust/Axum 约定、Cargo workspace 结构、测试约定、中文 commit）。为什么先做：主人 CLAUDE.md 明确"没有规范的工作空间不动手"。
2. 主人 review spec — 确认设计或提调整。为什么第二：review 是 writing-plans 的前置（brainstorming 流程的 user review gate）。
3. writing-plans 做 M0 实现计划 — review 通过后调用 superpowers:writing-plans，针对 M0 骨架生成可执行计划。为什么第三：依赖前两项。

## 7.x (archive) 之前接续的最短路径

（本会话为初始会话，无历史 §7 可归档。）
