# Spec Review — import 存量 skill 迁入 canonical 池设计（2026-08-14）

> 审查对象：`docs/superpowers/specs/2026-08-14-import-relocate-design.md`
> 审查基准：对代码逐一核对（`crates/core/src/{import,scope,symlink,install,paths,config}.rs`、`crates/cli/src/commands/import.rs`、`crates/server/src/routes/skills.rs`、CLAUDE.md §5/§6/§8）+ 交接记录 `docs/sessions/2026-08-10-skillkit.md` §1.2/§3.1/§5.1。
> 日期：2026-08-14
> 结论：**设计方向正确、行号声明高度准确，可进入实现，但须先修 1 个 P1（adopt_into_pool 对 symlink 类型 src 无防护）**，另有 2 个 P2 建议、1 个 P3 措辞。详见 §2。

## 1. 总体结论

- 行号/签名声明核对**全部通过**：import.rs（10 处行号）、scope.rs:57-72 迁移+dedup 范式、symlink.rs（4 处）、install.rs（3 处）、paths.rs:37-45、config.rs:26-44、cli import.rs:21-27、server import handler summary，逐字与现状一致（见 §3）。
- 架构决策合理：adopt_into_pool 是对 scope.rs:57-72 已验证模式的正确提取复用；relink 覆盖「改主循环触及不到的存量」这一真问题；顺序约束（adopt → registry save → 桥接）与 install.rs:45-52 对称，失败面分析成立。
- 与既有约束一致：迁入 `skillkit_skills_dir()`（`~/.skillkit/.agents/skills/`）不违反 CLAUDE.md §5 单版本模型与 §1.2「`~/.agents/skills/` 只放全局公共 skill、不写元数据」红线；uninstall 对 unmanaged 不删 canonical 的连带影响声明（§6）与 install.rs:61 + 测试 install.rs:87 吻合。
- **唯一阻断项**：§3.1 adopt_into_pool 对 src 是符号链接的情况没有防护，而主循环扫描 `~/.agents/skills/` 时 skip_symlink=false，会把符号链接当 src 传入，rename 符号链接会产出「canonical 是 symlink」的坏状态。须先补防护再实现。

## 2. 必须修正 / 需决策的问题

> 严重度分级：🔴 P0 必须修（计划级 bug，执行前改 plan）｜🟠 P1 需决策（与既有决策/代码语义矛盾）｜🟡 P2 建议修｜⚪ P3 不阻塞。
> 本文档为 design spec（无独立 plan），以下问题均在 spec 层。

### 🔴 P1 — adopt_into_pool 对 symlink 类型的 src 无防护，会产出「canonical 是 symlink」坏状态

- 现象：主循环 unmanaged 分支把 scan_dir 扫到的 canonical 路径直接传给 adopt_into_pool 作 `src`。但 scan_dir 对 `~/.agents/skills/` 分支传入 `skip_symlink=false`（import.rs:33-34），即该目录下的 symlink-to-dir 会被扫进 plan。§3.1 的 adopt_into_pool 对 `src` 是符号链接的情况没有任何前置判断：当池子无同名（target 不存在）时走 `std::fs::rename(src, &target)`——**rename 一个 symlink 只移动链接本身，target 变成指向原目标的 symlink，真实内容并未迁入池子**。结果 `canonical_path` 指向池子里一个符号链接，违反「canonical 是真实目录」模型，后续 computed_hash 计算（install_local hash 跳 symlink）/ uninstall / rescope 行为全部异常。
- 证据：
  - import.rs:33-34 `scan_dir(&paths.agents_skills_dir(), false, false, ...)`（第二个参数 skip_symlink=false）
  - import.rs:129 `if skip_symlink && p.is_symlink() { continue; }`——仅在 skip_symlink=true 时生效，agents 分支不生效
  - spec §3.1 adopt_into_pool 代码直接 `std::fs::rename(src, &target)`，无 src 类型判断
  - 对照 scope.rs:50-56 `real_canon` 用 `is_dir() && !m.file_type().is_symlink()` 显式过滤只取真实目录；spec §3.3 relink 也有「canonical 是 symlink → 跳过」。主循环 → adopt 路径独缺这层对称防护。
- 影响：用户在 `~/.agents/skills/` 手工放置指向别处的 symlink 时，import 后池中 canonical 是 symlink，数据模型被破坏。次要情形：skillkit 自建桥接 symlink 在 registry 缺记录时被重复 adopt（删了又重建，冗余但最终一致）。
- 修正建议：二选一——(a) adopt_into_pool 开头加 `if std::fs::symlink_metadata(src)?.file_type().is_symlink() { 调用方按 skip 处理或返回特殊标记 }`；(b) 主循环调用 adopt 前判断 canonical 是 symlink 则走 `report.skipped`（对齐 import.rs:129 与 relink 的「只迁真实目录」原则）。推荐 (b)，把判定留在调用方，adopt 保持纯迁移职责。
- 测试盲区：现有 import.rs 测试 `make_skill`（import.rs:186-194）全部建真实目录，验不出 symlink-src；scope.rs 有 `real_canon` 过滤的专项测试（scope.rs:227 `unmanaged_global_to_local_migrates_canonical`）但 import 侧无对应。须补：预置 `~/.agents/skills/<name>` 为指向外部真实目录的 symlink → import 后该条进 skipped，池子不出现同名 symlink-canonical。

### 🟡 P2 — relink 信任 registry.canonical_path，与 scope.rs「不信任 canonical_path 会漂移」原则存在未说明的张力

- 现象：scope.rs:45-46 明确注释「不信任 registry canonical_path——它可能漂移」，故 set_scope 扫物理位置（agents_link/claude_link）找回真实 canonical。而 §3.3 relink 完全基于 `registry.canonical_path` 判断 `!starts_with(skillkit_skills_dir())`，两种判定方式不一致。
- 证据：scope.rs:45-56（扫物理位置找回）；spec §3.3（按 registry canonical_path 遍历 + starts_with 判定）。
- 影响：当 registry.canonical_path 漂移到不存在路径时，relink 只能 dangling warn 跳过（放弃），不具备 scope.rs 的物理位置找回能力。import 场景下漂移 skill 本就是孤儿（§6 已声明留待手工清），放弃可接受——但 spec 未显式说明这一权衡，实现者/后续维护者易误以为 relink 与 scope.rs 判定等价。
- 建议：§3.3 补一句权衡说明——relink 以 registry 为遍历源（换取「canonical 被挪到 4 个扫描目录之外也能覆盖」的全面性），代价是放弃 scope.rs 的物理位置找回（漂移到不存在路径只能 warn 跳过）。无需改逻辑，仅补文档。
- 测试盲区：§8 新增 relink 测试覆盖了 dangling / symlink / 已入池三种边界，但没覆盖「registry canonical_path 与物理位置不一致（漂移到另一个真实目录）」这一 scope.rs 特别处理的场景——建议显式声明该场景归入 dangling 同等待遇（跳过 + warn），避免实现者猜测。

### 🟡 P2 — ImportReport 加字段应顺带补 import 的 `--json` schema 锁定测试

- 现象：CLAUDE.md §6/§8 把 `--json` schema 视为公开契约，其他命令（find/list/install-local/rescope）均有 `*_json_schema_locks_*` 测试锁定字段名。import 命令当前缺此类测试。§7 给 ImportReport 新增 relocated/relinked 后，§8 测试策略只列 import.rs 单测更新，未提新增 schema 锁定测试；§12 仅说「跑一次 --json 确认输出结构」，未纳入测试。
- 证据：CLAUDE.md:54（schema 公开契约）、:96（schema 锁定测试约定）；cli 现有 schema 锁定测试 skill.rs:207/249、install.rs:228、rescope.rs:119；spec §8 未列 import schema 锁定测试。
- 影响：新增字段无锁定测试守护，未来若再改 ImportReport 字段名无测试拦截，与 §6 契约意图相悖。
- 建议：§8 补一条——新增 `import_json_schema_locks_fields` 测试，断言 `--json` 输出含 `imported/unmanaged/reinstalled/skipped/relocated/relinked` 字段名（对齐 install.rs:228 `install_local_json_schema_locks_fields` 的写法）。
- 测试盲区：正是本条要补的缺口。

### ⚪ P3 — §3.5「同一文件系统 rename 原子」措辞在生产环境非绝对

- 现象：§3.5 断言「src 与 target 都在 `$HOME` 下（生产）……同文件系统，rename 原子」。生产环境 `$HOME` 下不同子目录通常同文件系统，但用户可能跨卷 mount（如把 `~/.skillkit` 挂到其他卷），此时跨文件系统 rename 返回 EXDEV 失败。
- 证据：spec §3.5；§6 已兜底「rename 失败（权限/跨文件系统）→ SkillkitError::Tool，canonical 未动」。
- 影响：逻辑已兜底，仅措辞过度断言。
- 建议：§3.5 把「同文件系统，rename 原子」弱化为「通常同文件系统（除非用户跨卷 mount），rename 原子；跨文件系统时按 §6 兜底报错，canonical 不动」。

## 3. 核对通过明细（供执行时对照，逐项已验证）

| Spec 声明 | 验证结果（文件:行号） |
|---|---|
| import.rs:89-108 unmanaged 分支（无 package） | 一致，import.rs:89 `} else if !dry_run {` … 108 |
| import.rs:63-109 主循环 | 一致，import.rs:63 `for (name, canonical, package) in plan` … 109 |
| import.rs:64-70 已登记 name 跳过 | 一致，import.rs:64-70（registered.contains → skipped） |
| import.rs:11-21 ImportReport | 一致，import.rs:11-21（当前无 relocated/relinked，系本次新增） |
| import.rs:104-107 dry_run 分支 | 一致，import.rs:104 `} else {` … 107 |
| import.rs:129 skip_symlink 约定 | 一致，import.rs:129 |
| scope.rs:57-72 / :60-64 迁移+dedup 范式 | 一致，scope.rs:57-72；adopt_into_pool 是对它的正确提取复用 |
| symlink.rs:10-22 ensure_global_claude | 一致，symlink.rs:10-22 |
| symlink.rs:29-31 指向正确即跳过（幂等） | 一致，symlink.rs:29-31 |
| symlink.rs:36-38 真实目录占位报 CanonicalCreate | 一致，symlink.rs:36-38 |
| symlink.rs:29-35 ensure_link 删旧链重建 | 一致，symlink.rs:29-35 |
| install.rs:45-47 先落盘 registry | 一致，install.rs:45-47 `upsert`+`save` |
| install.rs:50-52 install 后桥接 | 一致，install.rs:50-52 `if Global { ensure_global_claude }` |
| install.rs:61 uninstall 对 unmanaged 不删 canonical | 一致，install.rs:61 `if computed_hash.is_some()`；测试 install.rs:87 印证 |
| paths.rs:37-45 codex/cursor 历史私有目录注释 | 一致，paths.rs:37-44（注释明示「import 扫描用；新设计下 agent 直读 ~/.agents/skills/」） |
| skillkit_skills_dir vs agents_skills_dir 区分 | 一致，paths.rs:48-50（池子 `~/.skillkit/.agents/skills/`）vs :28-30（`~/.agents/skills/`） |
| config.rs:26-44 cursor/codex reads_agents_dir=true | 一致，config.rs:32-41 |
| cli import.rs:21-27 summary | 一致，cli import.rs:21-27（文案待按 §4 改） |
| server skills.rs import handler summary（§5 :340-357/:343-349） | 一致，import handler summary format 块文案与 CLI 同形（行号 ±2 内，措辞实现时定） |
| CLAUDE.md §5 canonical 单池子约束 | 一致，CLAUDE.md:43「canonical 物理存储只有一份」 |
| CLAUDE.md §6 --json schema 公开契约 | 一致，CLAUDE.md:54 |
| 主 spec §459 历史目录迁移后归档 | 交接 §1.2 单版本模型 + paths.rs:37 注释佐证 |
| §6 uninstall 连带影响（池子留孤儿 + 桥接 dangling） | 与 install.rs:58-71 unmanaged 分支语义吻合，声明准确 |
| §3.2 顺序 adopt→save→桥接 与 install 对称 | 一致，install.rs:45-52 同序 |

## 4. 修正建议的执行顺序

1. 先改 spec（`docs/superpowers/specs/2026-08-14-import-relocate-design.md`）：
   - §3.1 adopt_into_pool：补 src 类型防护（P1，主循环调用前判 symlink → skipped，或 adopt 内前置判断）。
   - §3.3：补 relink 与 scope.rs 判定方式的权衡说明（P2）。
   - §8：补 `import_json_schema_locks_fields` 测试条目（P2）。
   - §3.5：弱化「同文件系统」措辞（P3）。
2. 改完 spec 再进入 writing-plans / 实现，按 §3 的 TDD 任务展开（现有 4 个 import.rs 测试 + §8 新增项）。

## 5. 结论

修完 P1（adopt symlink 防护）即可安全进入实现；两个 P2 是补强（relink 权衡说明 + schema 锁定测试），P3 仅措辞。设计整体扎实：行号声明零偏差，adopt/relink/顺序约束都精准对齐既有 scope.rs/install.rs 模式，失败面与连带影响分析到位。
