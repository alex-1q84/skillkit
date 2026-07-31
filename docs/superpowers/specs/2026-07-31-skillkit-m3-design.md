# SkillKit M3 迁移打磨设计

- 日期：2026-07-31
- 状态：待评审
- 依赖：M2 已完成（四视图 + apply 闭环 + SSE，60 tests + 前端 e2e 4 用例全绿）
- 关系：本 spec 承接 `docs/2026-07-29-skillkit-design.md` §15 的 M3 三件事（import-existing / upgrade / Brewfile 打包），细化到可实施。

## 1. 目的与范围

M3 是「迁移打磨」：让 skillkit 能接住真实环境里已经存在的存量 skill（63 个在 `~/.agents/skills/`、9 个在 `~/.codex/skills/`、2 个在 `~/.cursor/skills/`、34 个在 `~/.claude/skills/`），并补齐版本升级与分发打包。

在范围：
- `skillkit import-existing`：扫描存量 skill 目录，识别 + 登记进 registry。
- `skillkit upgrade <id> | --all`：升级已安装 skill，检测 project `locked_shas` 冲突。
- mac-config justfile 打包（`install_skillkit` recipe）。
- GUI Skills 视图 unmanaged 标记 + 升级按钮。

不在范围（YAGNI / 未来）：
- skillkit 发布到 crates.io / GitHub release / brew tap（未发布状态，Brewfile 打包推迟，见 §6）。
- unmanaged skill 自动落地到项目（只登记展示，不落地，见 §2）。
- import-existing 的深度溯源（只认 `.git` remote url 一种形态，见 §3）。
- profile 继承、多用户、远程访问等（已在 M2 spec §13 排除）。

## 2. 数据模型扩展：unmanaged skill

现有 `SkillMeta` 是 `{id, name, source, scope, version, computed_hash, installed_at, canonical_path}`。M3 引入 **unmanaged skill**：存量目录无法溯源时，登记为「可见但不可升级」的 skill。

字段约定：

| 字段 | 值 | 说明 |
|------|-----|------|
| `id` | `<虚拟源>/<name>` | 虚拟源固定 `unmanaged`（如 `unmanaged/bb-review`） |
| `source` | `unmanaged` | 与真实源（skills.sh / 自定义源）区分 |
| `scope` | `global` | 存量在 `~/.agents/skills/` 等，agent 已直读 |
| `computed_hash` | `None` | 无版本锁 |
| `version` | `None` | 无版本信息 |
| `canonical_path` | 存量真实目录路径 | 如 `~/.agents/skills/bb-review` |

约束与影响：
- **upgrade 跳过** `computed_hash=None` 的 skill（不可升级，提示原因）。
- **uninstall 不删目录**（不是 skillkit 装的，只摘 registry 记录；避免误删用户手工放置的 skill）。
- **profile 可引用** unmanaged skill（GUI 归类展示）。
- **apply 兼容**：unmanaged 的 `canonical_path` 是真实目录，若被项目以 local scope 引用，apply 的 symlink/copy 落地逻辑天然可用（Landing 源是真实目录）。但 M3 首版**不自动把 unmanaged 落地到项目**——存量 agent 已直读，无需重复落地，YAGNI。
- **registry 仍是单存储**：unmanaged 与 managed 混在同一 registry.json，GUI/CLI 查询不分裂。

## 3. `skillkit import-existing`

```
skillkit import-existing [--json] [--dry-run]
```

### 3.1 扫描范围（主人确认：按 spec §15 扫全部）

| 目录 | 角色 | 处理 |
|------|------|------|
| `~/.agents/skills/` | 规范存储（63 个，agent 直读） | 无源 → unmanaged 登记 |
| `~/.claude/skills/` | Claude 落地（34 个，含 symlink → `~/.agents/skills/`） | **跳过 symlink**（已由 `~/.agents/skills/` 覆盖），只登记真实目录 |
| `~/.codex/skills/` | 历史私有（9 个） | unmanaged 登记 |
| `~/.cursor/skills/` | 历史私有（2 个） | unmanaged 登记 |

扫描目录清单在 `Paths` 增加对应方法（`codex_skills_dir()` / `cursor_skills_dir()`），可测试注入。

### 3.2 核心流程（`--dry-run` 只输出不写）

1. **发现**：扫描各目录，子目录含 `SKILL.md` 即视为一个 skill（目录名 = skill name）。
2. **去重**：
   - `~/.claude/skills/` 里的 symlink → 跳过（已由 `~/.agents/skills/` 覆盖）。
   - 已登记过同名 skill → 跳过（幂等重入）。
   - 同名出现在多个目录（如 `bb-review` 在 agents 和 codex 都有）→ 以 `~/.agents/skills/` 优先，其余跳过并警告（大概率同一 skill 的旧拷贝）。
3. **溯源**（尝试识别 package）：
   - 目录里有 `.git` → 可溯源，读 remote url 得 package（github shorthand / git url）→ **重装入池**（`npx skills add <package>`，走现有 `install` 流程，得 computed_hash，真正 managed、可升级）。
   - 无 `.git` → unmanaged（登记虚拟源 `unmanaged`）。
   - **只认这一种形态，不猜其他**（现实中绝大部分走 unmanaged——全局无 skills-lock.json、目录无 `.git`）。
4. **登记**：unmanaged 写 registry（虚拟源 `unmanaged`，computed_hash=None）；可溯源的走 `install` 重装入池子（managed，computed_hash 有值）。**注意：可溯源但未重装 = 无 hash = 仍是 unmanaged**（无法升级），所以「可溯源」必须配「重装」才成为 managed。
5. **输出**：人类模式打印汇总（导入 N 个，unmanaged M 个，重装 R 个，跳过 K 个）；`--json` 输出结构化数组 `[{name, source, canonical_path, managed}]`。

### 3.3 边界处理

- 无 `SKILL.md` 的目录 → 跳过（不是 skill）。
- 空目录 / 无权限目录 → 跳过并警告，不中断。
- 幂等：重复跑不重复登记、不重复输出。

## 4. `skillkit upgrade`

```
skillkit upgrade <id> | --all [--yes] [--json]
```

### 4.1 核心流程

```
1. 解析目标 id → registry 查 SkillMeta
   ├─ 未安装 → 报错「skill 未安装，先 skillkit install <id>」
   ├─ computed_hash=None（unmanaged）→ 跳过（"unmanaged skill 无版本锁，无法升级"）
   └─ 已安装 → 继续
2. 冲突检测：扫描所有 project 的 locked_shas
   └─ 有项目锁了旧版本且 `yes=false` → 返回 `Err(UpgradeBlocked { id, affected })`（列出受影响项目）
      ├─ CLI 层 catch：打印受影响项目 → 交互确认 y/n
      │  ├─ y → 以 yes=true 重调 upgrade_skill
      │  └─ n → 输出「已取消」退出码非 0
      └─ yes=true → 跳过确认直接升级（affected 记入 report）
3. 执行 npx skills update <skill>（复用 npx::update）
4. 重读 skills-lock.json 的 computed_hash → 更新 registry.computed_hash + installed_at
5. 输出：✓ 已升级 <id> old → new
```

### 4.2 语义要点

- **冲突检测的实际作用**：单版本模型下 upgrade 全局物理生效，`locked_shas` 无法锁版本——冲突检测的用途是「人类交互确认」：让用户知道升级会连带影响 N 个项目的基线；`--yes` 跳过确认给 agent/CI。
- **升级后 locked_shas 不自动改**——那是 `project apply` 的职责（apply 检测到 hash 漂移更新基线，或 `--frozen` 报错）。upgrade 只改 registry，单一职责。
- **不自动 apply 受影响项目**——YAGNI，让用户显式 `project apply`。
- **`--all` 容错**：逐 skill 升级，遇到 unmanaged / 未安装的跳过并统计，不中断。
- **全局 symlink 无需重建**——canonical 目录原地更新，symlink 仍指向同一 canonical。
- **--json**：单 skill 输出 `{id, old_hash, new_hash}`；`--all` 输出数组。

## 5. GUI 扩展（Skills 视图）

让 unmanaged skill 在 Skills 视图可见、可分类：

- **分类展示**：Skills 视图按 `scope` 分组（global/local）+ **unmanaged 标记**（`computed_hash=None` 显示「未托管」角标）。
- **升级按钮**：managed skill 卡片加「升级」按钮；unmanaged 不显示（不可升级）。
- **升级流程**：点升级 → htmx POST `/skills/{id}/upgrade` → 服务端调 core `upgrade_skill` → 返回刷新片段 → SSE 兜底刷新。

新端点：

| 端点 | 方法 | 作用 |
|------|------|------|
| `/t/<token>/skills/{id}/upgrade` | POST | 升级单个 skill（服务端走 core `upgrade_skill`） |

变更范围：`crates/server/src/routes/skills.rs`（加 upgrade 端点）、`crates/server/templates/skills.html` + fragments（unmanaged 标记 + 升级按钮）。core 复用 §4 的 `upgrade_skill`，无新公开 API。

## 6. mac-config 打包（justfile）

主人确认：**进 justfile，不进 Brewfile**。

原因：skillkit 是本地 workspace 项目，尚未发布（无 crates.io / GitHub release / tap formula）。cx/rtk 能走 `brew "cx"` / `brew "rtk"` 是因为它们已发布到远端 tap；skillkit 未发布前，Brewfile tap formula 是过度设计。与 mac-config 现有非 brew 工具（skvm / opencli / codebase-memory-mcp）的 `install_*` recipe 模式一致。

做法：在 mac-config justfile 加 recipe：

```make
# install/update skillkit
install_skillkit:
	cargo install --path /Users/mywo/lab/skillkit/crates/cli --locked --force
```

`cargo install --path <workspace>/crates/cli` 安装 skillkit-cli 的二进制 `skillkit`，`--locked` 用仓库锁的依赖版本，`--force` 覆盖旧版。Brewfile 不加 `brew "skillkit"`。

## 7. core 新增公开 API（lib.rs re-export）

| 函数 | 签名 | 用途 |
|------|------|------|
| `import_existing` | `fn import_existing(paths: &Paths, dry_run: bool) -> Result<ImportReport>` | import-existing 主流程 |
| `upgrade_skill` | `fn upgrade_skill(paths: &Paths, id: &str, yes: bool) -> Result<UpgradeReport>` | 升级单个 skill；`yes=false` 且锁冲突 → `UpgradeBlocked` |
| `UpgradeBlocked` | `{id, affected: Vec<String>}` | 冲突被拦截（CLI 层交互确认后重调） |
| `upgrade_all` | `fn upgrade_all(paths: &Paths, yes: bool) -> Result<Vec<UpgradeReport>>` | 升级全部（容错跳过 unmanaged） |
| `ImportReport` | `{imported: Vec<String>, unmanaged: Vec<String>, skipped: Vec<String>}` | import 结果 |
| `UpgradeReport` | `{id, old_hash, new_hash, affected_projects: Vec<String>}` | 升级结果 |

按 CLAUDE.md §7「core 公开类型一律在 lib.rs 完整 re-export」，新增类型补进 lib.rs。

## 8. 错误处理

遵循「反馈引导行动」：

- import-existing：无权限目录 → 跳过并警告（列出路径），不中断。
- upgrade：未安装 → `SkillkitError::SkillNotInstalled`（"先 `skillkit install <id>`"）；unmanaged → 跳过并说明（"无版本锁，无法升级"）。
- **冲突确认**：core 返回 `UpgradeBlocked {id, affected}`（不读 stdin），CLI 层打印受影响项目并 y/n 交互确认，n 则退出码非 0 不升级（`--yes` 跳过此环节）。
- `npx skills update` 失败 → `SkillkitError::Tool` 带 stderr（引导检查网络 / 源可达性）。

## 9. 测试策略

测试验证业务结果，不验证实现细节：

- **import-existing**（core 集成测试，tempdir 注入）：
  - 构造带 `SKILL.md` 的 `~/.agents/skills/foo` + `~/.codex/skills/bar` + `~/.claude/skills/baz`（symlink）+ 空目录 + 无 SKILL.md 目录。
  - 断言：agents/codex 真实目录登记为 unmanaged；symlink 跳过；空/无效目录跳过；`--dry-run` 不写 registry；重复跑幂等。
  - `.git` 目录 + remote url → 登记真实源（managed）。
- **upgrade**（core 集成 + 单元）：
  - 未安装 → 报错。
  - unmanaged（computed_hash=None）→ 跳过。
  - 冲突检测：project locked_shas 锁了旧 hash → 列出受影响项目；`yes=true` 跳过确认。
  - `--all` 容错：managed + unmanaged 混合 → 只升 managed，跳过统计。
  - npx 真跑（`#[ignore]`，类似 m0 端到端）：`install` 后 `upgrade`，断言 registry.computed_hash 更新。
- **GUI**：Skills 视图渲染 unmanaged 角标 + 升级按钮存在；`POST /skills/{id}/upgrade` 契约测试。
- **`--json` schema 锁定**：ImportReport / UpgradeReport 结构稳定（AI agent 依赖）。
- **CLI clap 解析**：`upgrade <id>` / `upgrade --all` / `import-existing --json` 解析测试。

## 10. 内部阶段拆分（供 writing-plans）

1. **core：unmanaged 数据模型** —— SkillMeta 支持 computed_hash=None 的 unmanaged 语义，registry 不变（字段本就 Option）。补 import/upgrade 需要的新类型（ImportReport/UpgradeReport）。
2. **core：import_existing** —— 扫描 + 去重 + 溯源 + 登记，TDD 红绿。
3. **core：upgrade_skill / upgrade_all** —— 复用 npx::update + 冲突检测 + 容错，TDD 红绿。
4. **CLI：import-existing / upgrade 命令** —— clap 解析 + --json + --yes + --dry-run。
5. **GUI：Skills 视图 unmanaged 标记 + 升级按钮 + upgrade 端点**。
6. **mac-config：justfile install_skillkit** —— 提交到 mac-config 仓库。
7. **文档同步** —— spec §11 命令、CLAUDE.md、交接记录更新。

## 11. 不在范围（YAGNI）

- skillkit 发布到 brew tap / crates.io（未发布，justfile 先顶着）。
- unmanaged 自动落地到项目（存量已直读，YAGNI）。
- import-existing 深度溯源（只认 `.git` remote url）。
- upgrade 自动 apply 受影响项目。
- profile 继承、多版本并存等既有排除项。
