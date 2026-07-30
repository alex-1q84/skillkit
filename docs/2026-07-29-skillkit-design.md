# SkillKit — AI Agent Skill 管理工具设计

- 日期：2026-07-29
- 状态：待评审
- 工具名：skillkit（CLI 二进制名同为 `skillkit`）

## 1. 背景与问题

当前 skill 生态分散在多个来源，缺乏统一管理：

| 来源 | 位置 | 数量 | 管理方式 |
|------|------|------|----------|
| npx skills 规范存储 | `~/.agents/skills/` | 64 个 | npx skills 管理 |
| Claude 全局 | `~/.claude/skills/` | 26 真实目录 + 7 个 symlink → `~/.agents/` | 混合：手放 + symlink 桥接 |
| Codex 全局 | `~/.codex/skills/` | 10 个 | 手写 just recipe 复制 |
| 插件系统 | `~/.claude/plugins/` | 4 个（superpowers 等） | Claude 原生，已有 version + gitCommitSha |

核心痛点：

1. `~/.claude/skills/` 里 26 个是手动放的真实目录，脱离任何版本管理，无法升级或追溯版本。
2. 跨 agent 同步（claude/codex）靠手写 just recipe（`_copy_skvm_skills_claude` / `_copy_skvm_skills_codex`），必然漂移。
3. 没有 profile 概念，所有项目看到同一坨全局 skill，无法按场景裁剪。
4. 团队项目中"共享但不入仓库"的 skill 没有干净的承载方式。

## 2. 目标与非目标

### 目标

- 设定 skill 安装源（skills.sh 生态、私有 git 仓库、本地路径）。
- 安装 skill 时记录并锁定版本（commit_sha），支持升级，版本信息集中管理。
- 按 profile 把 skill 组织成可复用的候选集，支持安装、升级、卸载。
- 按项目管理：注册或扫描项目，精确到逐个 skill 指定安装，应用 profile，执行幂等落地。
- 提供本地 web GUI 方便配置和查看全貌（源、skill、profile、项目）。
- 提供 CLI 方便 AI agent 操作，输出结构化 JSON。

### 非目标

- 不管 Claude 原生插件系统（superpowers、knowledge-copilot 等由 `installed_plugins.json` 管理的，有原生市场和版本管理，工具去碰会冲突）。
- 不管 `~/.claude/commands/`（自定义命令，非 SKILL.md 形式）。
- 不管理项目级 shared skill 的安装/升级/卸载（它由项目 git 自己管，skillkit 只做只读发现）。
- 不实现"同一 skill 多物理版本并存"（YAGNI，见第 16 节升级路径）。
- 不做远程/云端服务，只在本机运行。

## 3. 总体架构

技术栈：Rust + Axum，编译为单二进制。核心库（`core` crate）承载全部业务逻辑，CLI 和 web server 两个入口共享核心，无重复逻辑。前端用 React + Vite 构建，产物经 `rust-embed` 嵌入二进制，分发仍是单文件。

```
┌─────────────────────────────────────────────────────┐
│                  skillkit 二进制                     │
│  ┌───────────────────────────────────────────────┐  │
│  │            core 库 (Rust crate)                │  │
│  │  source / skill / version / profile / project │  │
│  │  registry / agent / symlink / copy            │  │
│  └──────────────────┬────────────────────────────┘  │
│                     │                                │
│   ┌─────────────────┼─────────────────┐             │
│   │                 │                 │             │
│ ┌─▼──┐         ┌────▼────┐                       │
│ │ CLI │         │ Axum    │                       │
│ │入口 │         │ web srv │                       │
│ └────┘         └─────────┘                       │
│  skillkit <cmd>  skillkit serve                    │
│  (AI agent)      浏览器访问 localhost:PORT          │
└─────────────────────────────────────────────────────┘
```

进程模型：无常驻 daemon。CLI 直接调用 core 库执行；`skillkit serve` 启动 Axum web server，同样直接调用 core 库。CLI 与 web server 是两个独立进程，**状态实时性靠文件监听打通**：任一方写 `~/.skm/` 状态文件后，server 用 file watcher（`notify` crate）感知变化并经 SSE 推给浏览器，不依赖 CLI 主动通知 server、也不引入 daemon 生命周期管理。并发写同一配置时，用文件锁（`~/.skm/` 下的写时锁）避免冲突（锁粒度见 §13）。

选 Rust 的理由：CLI 会被 AI agent 高频调用（每次会话可能多次），单二进制毫秒级启动、零运行时依赖，且能纳入 mac-config 的 Brewfile 统一分发，与现有 cx/rtk 工具链一致。

## 4. 与 npx skills 的边界

`npx skills` 仅承担一个职责：从 skills.sh 源下载 skill 到 `~/.agents/skills/`（即下载动作）。其余全部由 skillkit 独立实现：

- skillkit 调用 `npx skills add <skills.sh 源> -g` 完成下载。
- 下载后 skillkit 接管：登记到 `~/.skm/registry.json`，记录版本，按 profile 落地到各 agent 目录。
- 非 skills.sh 源（私有 git、本地路径）由 skillkit 自己 `git clone` 或复制到 canonical 存储，不经 npx skills。

skillkit 不依赖 npx skills 的内部状态，版本记录采用自己的 `registry.json` 格式（参考 Claude 插件 `installed_plugins.json` 的 `version` + `commit_sha` + `installed_at` 字段）。

## 5. Skill 分类与管理职责

skillkit 把 skill 分为三类，管理职责不同：

| 类型 | canonical 存储 | skillkit 角色 |
|------|------|------|
| 全局公共 | `~/.agents/skills/` | 全权管理（源、版本、profile、apply） |
| 项目 local | `~/.skm/skills/` | 全权管理（源、版本、profile、apply 到项目 local 目录） |
| 项目 shared | `<project>/<agent>/skills/` | 只读发现（扫描展示，与 local 对照，不安装/升级/卸载） |

语义说明：

- 全局公共 skill 是所有项目、所有 agent 共享的基座，装一次全局生效。
- 项目 local skill 是项目特定的、不入仓库的 skill，从外部源安装，由 skillkit 管理版本和落地。
- 项目 shared skill 是项目自有的、随 git 仓库分发的 skill，由项目自身（git）管理，skillkit 只扫描展示，方便与 local skill 对照查看。

## 6. 存储模型

### 6.1 关键约束

`~/.agents/skills/` 是通用 AI agent skills 加载目录，除 Claude Code 外大部分 agent（Cursor、OpenCode、Codex、Gemini CLI 等）都直接从此目录加载 skill。因此该目录只用于全局公共 skill 的存放，不得挪用为项目级 skill 的暂存区，也不得在里面放元数据文件（避免被 agent 误扫描）。所有 skillkit 元数据统一收在 `~/.skm/` 下。

### 6.2 全局布局

```
~/.skm/
  config.toml                    # agent 列表 + 各 agent 能力 + web 端口
  sources.toml                   # 安装源注册表
  registry.json                  # 所有已安装 skill 元数据，以 id 为 key
  skills/<skill-name>/           # 项目 local skill 的集中 canonical（单版本）
  profiles/<name>.toml           # 可复用 profile（声明式，可分享）
  projects/<project-id>.toml     # 项目实例元数据（个人本地，不入库）
  .lock                          # 写时文件锁（按文件粒度：registry 写、各 project 写分别加锁，见 §13）

~/.agents/skills/<skill-name>/   # 全局公共 canonical（Cursor 等直读，零配置）
~/.claude/skills/<skill-name>    # symlink → ~/.agents/skills/<skill-name>/（仅 Claude 需要桥接）
```

项目 local skill 的 canonical 集中放在 `~/.skm/skills/` 而非每个项目各放一份，这样同一 skill 被多个项目引用时只占一份磁盘，升级只改一处。

### 6.3 项目目录布局

local skill 与 shared skill **同级平铺**在 `<agent>/skills/<skill-name>/`（Claude Code 只发现一层目录下的 skill，`<agent>/skills/<分类>/<skill>/` 子目录不发现；详见决策 12）。两者都是 `<skill-name>/SKILL.md`，区分不靠路径而靠 skillkit 的落地清单：

```
<project>/
  .claude/skills/<skill-name>/        # shared 真实文件（git 提交，skillkit 只读）或 local symlink → ~/.skm/skills/<skill-name>/
  .cursor/skills/<skill-name>/        # shared 真实文件 或 local copy 自 ~/.skm/skills/<skill-name>/
  .codex/skills/...                   # 同理
```

**local skill 的 git 忽略用 git 自带的本地忽略文件 `<project>/.git/info/exclude`，不碰项目 `.gitignore`**。`apply` 把当前 local skill 清单写入 exclude（每行一条 `<agent>/skills/<skill-name>`）；该文件天然本地、不入库，每个开发者 clone 后跑自己的 apply 自动生成，团队成员互不冲突。示例：

```
# skillkit managed — local skills (per-developer, not committed)
.claude/skills/frontend-design
.cursor/skills/frontend-design
```

skillkit 不在项目目录写入自己的配置文件（项目元数据全部放在 `~/.skm/projects/<project-id>.toml`，保持 local 配置个人本地、不入库）。非 git 项目（无 `.git/info/exclude`）：local 直接平铺，无需忽略。边界：local 与 shared 同名时 shared（已在 git）优先、local 跳过并警告；apply 前若 local 已被 `git add`，exclude 对已追踪文件无效，apply 检测到并提示 `git rm --cached`（见 §13）。

### 6.4 project-id 生成规则

`project-id = <目录名>-<绝对路径 SHA256 前 6 位>`，例如 `mac-config-a1b2c3`。同一绝对路径生成稳定 id，便于幂等注册。

## 7. Agent 能力矩阵与落地策略

不同 agent 对 symlink 的支持和 skill 加载目录不同，落地策略据此选择：

| agent | 直读 `~/.agents/skills/` | 支持 symlink | 全局公共落地 | 项目 local 落地 |
|-------|:---:|:---:|------|------|
| Claude Code | 否 | 是 | symlink `~/.claude/skills/<skill>` → `~/.agents/skills/<skill>` | symlink `<project>/.claude/skills/<skill>` → `~/.skm/skills/<skill>` |
| Cursor | 是 | 否 | 无需操作（直读 `~/.agents/skills/`） | copy `~/.skm/skills/<skill>` → `<project>/.cursor/skills/<skill>/` |
| OpenCode / Codex / Gemini | 是 | 是 | 无需操作（直读） | symlink 或 copy 均可，默认 symlink |

agent 列表和能力在 `~/.skm/config.toml` 声明，新增 agent 只改配置不改代码。Cursor 因不支持 symlink，项目 local skill 用 copy 兜底，apply 时按 canonical 内嵌的 commit_sha 检测副本是否过期，过期则重新 copy。全局层面这些 agent 直读 `~/.agents/skills/`，不再依赖各自的历史私有目录（`~/.codex/skills/`、`~/.cursor/skills/` 等）；存量 skill 在 M3 迁移时导入（见 §15）。

## 8. 数据模型

### 8.1 Skill id

id 格式为 `<source-name>/<skill-name>`，例如 `skills.sh/frontend-design`、`team-private/tdd`。这样不同源的同名 skill 不会冲突，且 id 本身可读。id 是 skill 在 profile、project、registry 之间引用的唯一标识。

### 8.2 Source（安装源）— `~/.skm/sources.toml`

```toml
[[source]]
name = "skills.sh"
type = "skills-sh"          # 走 npx skills 下载到 ~/.agents/skills/

[[source]]
name = "team-private"
type = "git"
url = "git@github.com:org/team-skills.git"
ref = "main"                 # 默认拉取分支或 tag

[[source]]
name = "dc"
type = "git"
url = "ssh://git@bitbucket.rd.800best.com:7999/datawarehouse/datacenter-skills.git"
ref = "main"
skills_dir = "skills"        # skill 在仓库的 skills/ 子目录下（一仓库多 skill）；省略=skill 在仓库根

[[source]]
name = "my-local"
type = "local"
path = "~/my-skills"
```

source 类型三种：`skills-sh`（走 npx skills）、`git`（任意 git URL，含私有仓库，依赖本地已配置的 SSH key 或 git credential）、`local`（本地路径）。`skills_dir`（可选，git/local 源）：skill 在仓库中的子目录，用于一个仓库含多个 skill 的场景（如团队 `datacenter-skills` 仓库 `skills/` 下有多个 skill）；省略时 skill 内容直接在仓库根。install 时按 `<skills_dir>/<skill-name>` 定位并平铺到 canonical，保持 Claude 可发现的单层结构（避免 `skills/` 中间层）。

### 8.3 Skill 元数据 — `~/.skm/registry.json`

```json
{
  "skills.sh/frontend-design": {
    "name": "frontend-design",
    "source": "skills.sh",
    "scope": "global",
    "version": "1.2.0",
    "commit_sha": "abc1234",
    "installed_at": "2026-07-29T10:00:00Z",
    "canonical_path": "~/.agents/skills/frontend-design"
  },
  "team-private/tdd": {
    "name": "tdd",
    "source": "team-private",
    "scope": "local",
    "version": "0.3.1",
    "commit_sha": "def5678",
    "installed_at": "2026-07-29T10:05:00Z",
    "canonical_path": "~/.skm/skills/tdd"
  }
}
```

字段说明：

- `scope`：skill 固有属性，`global`（canonical 在 `~/.agents/skills/`）或 `local`（canonical 在 `~/.skm/skills/`）。由 `install` 时的 `--scope` 决定，存在 registry，不在 profile/project 重复。同一 skill 在 registry 只有一条记录、scope 固定，不能同时以 global 和 local 两个 scope 存在（单版本模型约束，见第 16 节）。
- `commit_sha`：版本锁依据，用于冲突检测和可复现安装。
- `canonical_path`：物理存储位置，apply 时 symlink/copy 的源头。

### 8.4 Profile（粗分类候选集）— `~/.skm/profiles/<name>.toml`

```toml
name = "frontend"
description = "前端开发场景的 skill 候选集"
skills = [
  "skills.sh/frontend-design",
  "skills.sh/dataviz",
  "skills.sh/canvas-design",
]
```

profile 只存 skill id 列表，不重复 source/scope/version 等信息（这些在 registry 里）。profile 是"这类场景可能用到的 skill 清单"，可提交到共享仓库让团队复用。profile 主要承载 local skill 的组合（per-project 生效的部分）；global skill 是全局基座，通常单独 `install` 管理，不依赖 profile 反复引用，但 profile 也允许引用 global skill（apply 时幂等确保其全局存在）。

### 8.5 Project（项目实例）— `~/.skm/projects/<project-id>.toml`

```toml
name = "mac-config"
path = "/Users/mywo/lab/mac-config"
agents = ["claude-code", "cursor"]
applied_profiles = ["frontend", "base"]
installed_skills = [
  "skills.sh/frontend-design",
  "skills.sh/dataviz",
]

[locked_shas]
"skills.sh/frontend-design" = "abc1234"
```

字段说明：

- `applied_profiles`：组织维度，记录项目关联了哪些 profile，用于 GUI 分组展示和批量操作入口。
- `installed_skills`：apply 的唯一事实依据，精确到每个 skill，是所应用 profile 候选集的子集选择。
- `locked_shas`：项目上次 apply 时各 skill 的 commit_sha 快照（变更基线）。canonical 升级后与 registry.commit_sha 比对，用于检测漂移和提示受影响项目。**注意：单版本模型下 canonical 物理只有一份，此字段不是多版本锁**——它无法让项目停在旧版本，作用是让 canonical 的变更被感知（见 §10.3）。

## 9. Profile 与 Project 的分工

| 维度 | 字段 | 作用 |
|------|------|------|
| 组织/候选 | `applied_profiles` | 标识项目关联的 profile，用于 GUI 分组和批量入口 |
| 安装/事实 | `installed_skills` | apply 的唯一依据，精确到每个 skill |

操作语义：

- `skillkit project apply-profile <project> <profile>`：把 profile 的全部 skill 批量加入 `installed_skills`（初始选择，用户可随后逐个移除）。
- `skillkit project add-skill <project> <id>` / `remove-skill <project> <id>`：精确增删单个 skill。
- profile 新增 skill 不会自动装到项目——用户必须显式选择，符合"精确控制"语义。

这样 profile 是"粗分类 + 批量操作入口"，project 的 `installed_skills` 是"精确事实"，两者职责分明。

## 10. apply 机制

`skillkit project apply <project>` 是核心命令，让项目各 `<agent>/skills/` 下 skillkit 管的 local skill 与 `installed_skills` 声明一致（shared 与用户手放的不碰）。流程幂等，可重复执行无副作用。

### 10.1 操作语义三层

| 操作 | 作用域 | 干什么 |
|------|------|------|
| `install` | canonical 仓库 | 从源拉取 skill 到 canonical 存储，登记 registry |
| `project add-skill` | 项目声明 | 把 skill id 加入 `installed_skills`（只改声明，不落地） |
| `project apply` | 落地 | 按 `installed_skills` 幂等同步到 agent 目录（实际生效） |

apply 按 skill 的 scope 分两条路径：

- **scope=global**：install 时已全局落地，apply 只做幂等检查（确保 canonical + Claude symlink 在位），**不在项目目录落地**——进 `installed_skills` 是为了声明"该项目依赖这个全局基座"，不产生 per-project 副作用。
- **scope=local**：按 §10.2 流程在项目 `<agent>/skills/` 落地（symlink/copy + `.git/info/exclude`）。

### 10.2 apply 流程

```
输入: project.toml 的 installed_skills + agents
  │
  ▼
1. 解析 installed_skills，查 registry 得每个 skill 的 scope + canonical_path + commit_sha
   ├─ skill 未安装 → 报错，提示先 skillkit install <id>
   └─ locked_shas[id] 与 registry.commit_sha 不一致 → 标记版本冲突
  │
  ▼
2. 计算目标状态：每个 agent 应有哪些 skill 落地
  │
  ▼
3. 扫描现状：<project>/<agent>/skills/ 下现有的 symlink 和目录，按 project.toml 的 local 落地清单区分 skillkit 管的 local 与 shared（git 真实文件，不碰）
  │
  ▼
4. diff 并幂等执行：
   ├─ 该有但没有 → 建立（支持 symlink 的 agent 建 symlink，Cursor 等 copy）
   ├─ 不该有但有 → 删除（仅删 skillkit 管的 local；symlink 删链接，copy 删目录；shared 不碰）
   └─ 版本冲突 → 默认以 canonical 为准更新 locked_shas；--frozen 模式报错不动
  │
  ▼
5. 重写 <project>/.git/info/exclude：按当前 local 落地清单更新忽略项（新增 local 加入、已移除 local 删除），保持 git 不追踪 local
  │
  ▼
6. scope=global 的 skill 落地是全局的（确保 ~/.agents/skills/ 存在 + ~/.claude/skills/ symlink），
   不 per-project，install 时即生效
```

### 10.3 版本冲突检测

两处触发：

- `skillkit upgrade <id>` 时扫描所有 project 的 `locked_shas`，若有项目锁了不同版本，列出受影响项目并警告，需 `--yes` 才继续。
- `skillkit project apply` 时发现 canonical 版本与 project 锁不一致，默认以 canonical 为准更新锁，`--frozen` 模式报错退出不动。

单版本模型下 canonical 物理只有一份，升级后所有项目物理同步更新——`locked_shas` 无法让项目停在旧版本，作用是让变更被感知：apply 时发现 canonical 与记录的基线不一致，提示该 skill 已升级（默认以 canonical 为准更新基线，`--frozen` 报错）。冲突检测保证升级不会默默发生而无人知晓。

## 11. CLI 命令

命令 git-style 分组，全部支持 `--json` 输出，危险操作默认交互确认、`--yes` 跳过。

```bash
# 源管理
skillkit source add <name> <type> [url|path] [--ref main]
skillkit source list [--json]
skillkit source remove <name>

# skill 安装到 canonical
skillkit install <id> [--scope global|local]      # global→~/.agents/skills/, local→~/.skm/skills/
skillkit uninstall <id>
skillkit upgrade <id> | --all
skillkit list [--scope global|local] [--json]

# profile（粗分类候选集）
skillkit profile create <name>
skillkit profile add-skill <profile> <id>
skillkit profile remove-skill <profile> <id>
skillkit profile list [--json]

# project（精确管理）
skillkit project add <path>                        # 注册项目，生成 project-id
skillkit project scan <dir> [--depth 3]            # 扫描发现项目（只列出：路径、project-id、检测到的 agent），不自动注册；确认后用 project add 注册
skillkit project list [--json]
skillkit project apply-profile <project> <profile> # 批量灌入 installed_skills
skillkit project add-skill <project> <id>          # 精确加单个
skillkit project remove-skill <project> <id>       # 精确删单个
skillkit project apply <project>                   # 幂等落地
skillkit project status <project> [--json]         # 输出该有/缺/多/冲突的 diff

# 迁移
skillkit import-existing                           # 扫描导入现有 skill（M3）

# web
skillkit serve [--port 7317]
```

AI agent 友好性：

- 全命令支持 `--json`，`project status --json` 输出 `{expected, missing, extra, conflicts}` 结构，供 agent 决策。
- 幂等可重入：重复 `apply` 无副作用，agent 可放心重试。
- 非交互：`--yes` 跳过确认，适配 CI/agent；人类模式默认交互确认危险操作。
- `project status` 是 agent 的感知接口，`project apply` 是执行接口，两者形成闭环。

## 12. GUI 范围

`skillkit serve` 启动 Axum web server，浏览器访问 localhost。四大视图对应"查看全貌 + 配置"：

| 视图 | 内容 | 核心操作 |
|------|------|------|
| Sources | 安装源注册表 | 增删源、浏览源内可用 skill |
| Skills | registry 总览（global/local 分类、版本、来源） | 搜索筛选、install/upgrade/uninstall |
| Profiles | profile 列表 + 每个 profile 的 skill 组成 | 勾选拖拽组装 profile、创建新 profile |
| Projects | 项目卡片：applied_profiles + installed_skills + shared(只读) + status | 应用 profile、精确增删 skill、一键 apply、查看 diff |

技术细节：

- 前端 React + Vite，构建产物 `rust-embed` 嵌入二进制。
- 后端 Axum REST API + SSE 推送（apply 进度、status 变化实时刷新）。
- localhost 绑定 + 随机 token 防其他进程误访问，无需登录。

GUI 价值是总览和可视化配置，CLI 价值是 AI agent 操作和脚本化，两者共享 core 库保证数据一致。

## 13. 错误处理

原则：反馈引导行动，不只报告失败。

| 场景 | 处理 |
|------|------|
| 源不可达（网络/认证失败） | 区分网络失败与认证失败，引导"检查 SSH key 或网络"，不静默跳过 |
| skill 未安装就 add-skill/apply | 报错并提示 `skillkit install <id>`，绝不静默跳过 |
| dangling symlink 或 canonical 丢失 | apply 时检测，重建或提示重装 |
| 版本冲突（多项目锁不同版本） | 列出受影响项目，`--frozen` 报错退出，默认交互决策 |
| Cursor copy 副本过期 | canonical 内嵌 commit_sha 标记，apply 时比对，过期则重新 copy |
| CLI 与 web server 并发写 | 文件锁粒度到单个文件（registry 一把、每个 project 各一把），互不阻塞；读操作（status/list）不抢锁；写锁带超时，超时按冲突报错而非死等 |
| 项目目录无写权限 | 报错退出，不降级静默 |
| local 与 shared 同名 | shared（已在 git）优先，local 跳过落地并警告，列出冲突 skill |
| local skill 已被 git 追踪 | `.git/info/exclude` 对已追踪文件无效，apply 检测到后提示 `git rm --cached <path>`，不静默跳过 |

## 14. 测试策略

测试验证业务结果（"apply 后项目能加载到正确 skill"），不验证实现细节（"调了 symlink 函数"）。

- 单元测试：core 纯逻辑（registry 解析、profile 合并、diff 计算、冲突检测、id 生成、project-id 生成）。
- 集成测试：用 tempdir 模拟整个 `~/.skm` + `~/.agents` + 项目目录，跑 install → apply 全流程，断言 symlink/copy 正确落地。
- 多 agent 路径：分别覆盖 Claude（symlink）和 Cursor（copy）两条落地路径。
- 幂等测试：重复 apply 断言零变化、零副作用。
- 冲突场景：多项目锁不同版本、dangling symlink、源失效。
- CLI schema 稳定性：`--json` 输出 schema 锁定测试（AI agent 依赖稳定结构）。
- 真实 git：用本地 bare repo 测 git 操作，不 mock。

## 15. 里程碑

每个里程碑独立可验证、可交付。

### M0 骨架

- core 库 + CLI 框架 + 配置和 registry 读写。
- source 管理（add/list/remove）。
- install/uninstall（git clone 到 canonical，skills.sh 源调 npx skills）。
- 全局 skill 的 Claude symlink 桥接。

交付价值：能装 skill、Claude 能用。

### M1 闭环

- profile 管理（create/add-skill/remove-skill/list）。
- project 注册（add）和扫描（scan）。
- project apply-profile / add-skill / remove-skill。
- project apply 幂等落地（Claude symlink + Cursor copy）。
- project status diff 输出（含 `--json`）。

交付价值：核心 CLI 闭环可用，达成最初目标。

### M2 GUI

- `skillkit serve` + Axum API。
- 四大视图（Sources/Skills/Profiles/Projects）。
- SSE 实时推送。

交付价值：可视化总览与配置。

### M3 迁移打磨

- 扫描导入现有 skill：`~/.agents/skills/`、`~/.claude/skills/`，以及各 agent 的历史私有全局目录（`~/.codex/skills/`、`~/.cursor/skills/` 等，按 `config.toml` 声明的 agent 扫描）。能确定源的标记版本、导入 `~/.agents/skills/`（让对应 agent 直读）；无法溯源的标记 unmanaged，GUI 里归类到 profile。新设计下这些 agent 直读 `~/.agents/skills/`，历史私有目录不再作为落地目标，迁移后可归档。
- 版本锁和冲突检测完善。
- Cursor copy 一致性保证。
- 打包进 mac-config Brewfile。

交付价值：上手即用、生产就绪。

## 16. 未来扩展（YAGNI 备忘）

当前不做但预留升级路径的事项：

- 同一 skill 多物理版本并存：若将来确有"项目 A 用 v1、项目 B 用 v2"需求，把 canonical 改为按版本分目录（`~/.skm/skills/<skill-name>/<version>/`），symlink 指向对应版本。现有抽象不破坏。
- 更多 agent 支持：config.toml 声明 agent 能力即可，不改代码。
- profile 继承（一个 profile 继承另一个）：目前 YAGNI，需要时再加。
