# Spec Review 第 2 轮 — import 存量 skill 迁入 canonical 池设计（2026-08-14）

> 审查对象：`docs/superpowers/specs/2026-08-14-import-relocate-design.md`（review 第 2 轮修订版）
> 审查基准：对代码逐一核对（`crates/core/src/{import,scope,symlink,install,paths,config,registry,error}.rs`、`crates/cli/src/commands/{import,install,rescope}.rs`、`crates/server/src/routes/skills.rs`、`CLAUDE.md` §5/§6/§8、`README.md`）+ 第 1 轮 review `docs/review/2026-08-14-import-relocate-design-spec-review.md`。
> 日期：2026-08-14
> 结论：**第 1 轮 P1/P2/P3 均已正确落实，本轮无新增 P0/P1 阻断项，可进入实现。** 3 个 P2 建议在实现前消化（补桥接对 dangling canonical 的作用域 / 错误变体名 / 跨目录同名中断）。详见 §2。

## 1. 总体结论

- 第 1 轮 4 条问题均已落实且核对无误：P1（adopt symlink 防护）→ §3.2 步骤 0 在 unmanaged 分支拦截 symlink canonical，与 scan_dir agents 分支 `skip_symlink=false`（import.rs:34）的实际扫描行为吻合，逻辑自洽；P2（relink 权衡）→ §3.3 末段补「与 scope.rs 的判定差异」声明，准确描述了「信任 canonical_path 换覆盖面」的取舍；P2（schema 锁定测试）→ §8 补 `import_json_schema_locks_fields`，对齐 install.rs:228 写法与 CLAUDE.md:96 约定；P3（rename 措辞）→ §3.5 弱化为「通常同文件系统，跨卷 mount 时 EXDEV 兜底」。
- 行号/签名声明本轮逐项抽验全部通过（见 §3）。5 处修改未引入错误引用。
- 核心设计扎实：adopt_into_pool 是对 scope.rs:57-72 的正确提取；主循环 adopt→registry save→桥接（§3.2）与 install.rs:45-52 同序对称；relink 覆盖「改主循环触及不到的存量」是真问题，按 registry 遍历比复用主循环更全面。
- 新发现 3 个 P2（均非阻断，建议实现前修，否则会产出有害状态或误导实现者）：① §3.3 补桥接「无论 canonical 是否刚归池，都调 ensure_global_claude」对 dangling/symlink canonical 会建出**自指或悬空 symlink**（典型场景：存量 agents 扫描的 unmanaged skill 被用户删后 canonical 漂空，补桥接用 `~/.agents/skills/<name>` 既当 target 又当 link 建自指环）；② §6/§7 称 rename 失败映射 `SkillkitError::Tool`，实际 §3.1 代码用裸 `?` 经 `#[from]` 映射成 `SkillkitError::Io`（error.rs:25-26）；③ 跨目录同名（agents+claude 均真实目录）在主循环 adopt 后建 claude 桥接时撞 CanonicalCreate 中断，与 §6「dedup 只登记首个、其余成孤儿」承诺矛盾。

## 2. 必须修正 / 需决策的问题

> 严重度分级：🔴 P0 必须修（计划级 bug）｜🟠 P1 需决策（与代码语义矛盾）｜🟡 P2 建议修｜⚪ P3 不阻塞。
> 本文档为 design spec（无独立 plan），以下问题均在 spec 层。

### 🟡 P2 — §3.3 补桥接对 dangling/symlink canonical 无条件执行，会建出自指/悬空 symlink

- 现象：§3.3 补桥接写「无论 canonical 是否刚归池，都调 ensure_global_claude」，其理由专门针对「canonical 已在池、桥接缺失」的中间态——这没问题。但「无论」一词把 dangling/symlink 两种 归槽 skip 的情形也带进了补桥接。对 dangling canonical，归槽 warn 跳过后 canonical_path 未更新，补桥接仍以这个不存在的路径为 target 调 ensure_global_claude。
- 证据：
  - spec §3.3 归槽 bullet：「canonical 不存在（dangling）→ warn 跳过」；补桥接 bullet：「无论 canonical 是否刚归池，都调 ensure_global_claude」——两条对 dangling 的处置冲突（归槽跳过、补桥接仍跑）。
  - 典型 canonical_path：old import 对 agents 扫描的 unmanaged 记 `canonical_path = ~/.agents/skills/<name>`（import.rs:99 记扫描路径）。补桥接里 `canonical = ~/.agents/skills/<name>`、`agents_link = paths.agents_skills_dir().join(name) = ~/.agents/skills/<name>`（symlink.rs:14-15），**target 与 link 同一路径**。
  - ensure_link（symlink.rs:25-45）对该路径：read_link 失败（已被删）、exists 为 false → 直落 `symlink(target, link)` → 建出 `~/.agents/skills/<name> → ~/.agents/skills/<name>` 自指环。若 canonical 在 claude 路径漂空，则建出 agents→claude→agents 互指环。
- 影响：自指/互指 symlink 会让 cursor/codex 等扫描 `~/.agents/skills/` 时拿到 ELOOP（追链递归），skill 加载失败。触发条件不算极窄——old import 登记的 agents 扫描 unmanaged（典型 canonical_path 就是 agents 路径），用户删掉该目录后跑新 import 即触发。
- 修正建议：补桥接加「canonical 已在池」前置条件。最简形式：补桥接仅当 `meta.canonical_path.starts_with(skillkit_skills_dir())` 时执行（归槽成功会更新 canonical_path 指池、本就在池的也满足；dangling/symlink 归槽跳过后 canonical_path 仍指原位 → 不满足 → 跳过补桥接，与归槽的 warn 跳过一致）。把 §3.3「无论 canonical 是否刚归池，都调」改为「若 canonical 已在池（含刚归槽），调」。
- 测试盲区：§8 relink 边界测试列了 dangling/symlink/已入池三种，但没断言「dangling 时补桥接不建 symlink」。须补：预置 canonical 漂空（如 `~/.agents/skills/<x>` 已删）的 unmanaged → import 后 relink warn 跳过，且 `~/.agents/skills/<x>`、`~/.claude/skills/<x>` 均不出现新建 symlink（验无自指/悬空桥接）。

### 🟡 P2 — §6/§7 称 rename 失败映射 SkillkitError::Tool，实际是 Io

- 现象：§6 错误表「rename 失败（权限 / 跨文件系统）→ SkillkitError::Tool」、§7「无新 error 变体（复用 CanonicalCreate / Tool）」。但 §3.1 的 adopt_into_pool 代码对 `std::fs::rename / create_dir_all / remove_dir_all` 用裸 `?`，而 error.rs:25-26 是 `Io(#[from] std::io::Error)`——裸 `?` 经 `#[from]` 映射成 `SkillkitError::Io`，不是 `Tool`。
- 证据：
  - error.rs:22-23 `Tool { message }`（无 `#[from]`，需手动 `.map_err()`）；error.rs:25-26 `Io(#[from] std::io::Error)`（`?` 自动走这条）。
  - §3.1 代码 `std::fs::rename(src, &target)?` 无 map_err → Io。对比 symlink.rs:33-35、41-43 对 remove_file/symlink 手动 `.map_err(|e| SkillkitError::Tool { .. })`——symlink 侧包成 Tool，adopt 侧裸 `?` 走 Io，两边风格不一致。
  - scope.rs:64/69（adopt 复制的范式源）同样是裸 `?` → Io。
- 影响：行为本身正确（`?` 向上抛、adopt 在 upsert+save 之前、canonical 未动），但变体名错。实现者若照 §6 字面加 `.map_err(|_| SkillkitError::Tool { .. })`，会丢掉 EXDEV「Cross-device link」等原始 io 信息（UX 变差），且与 §3.1 代码矛盾；若照 §3.1 用裸 `?`，又与 §6「Tool」声明矛盾。spec 内部打架。
- 修正建议：§6/§7 把 rename/FS 失败的变体改称 `SkillkitError::Io`（与 §3.1 裸 `?` 一致，保留原始 io 信息）。§7「复用 CanonicalCreate / Tool」改为「复用 CanonicalCreate；FS 错误走既有 Io（#[from]）」。无需改代码，仅修文档。
- 测试盲区：§8 未列 rename 失败的错误变体断言。若要锁定，可补一条跨文件系统场景（tempdir 下不同 mount 点）断言返回 `SkillkitError::Io`——但 macOS 测试环境难造跨卷，优先级低，可不补。

### 🟡 P2 — 跨目录同名（agents+claude 均真实目录）触发 CanonicalCreate 中断，与 §6 优雅 orphan 承诺矛盾

- 现象：§6「跨目录同名副本」行承诺「import dedup 只登记首个（import.rs:64-70），其余副本留原地成孤儿」。但本轮新增的桥接步骤会在处理首个副本时撞上第二个副本的真实目录占位。具体：同名 `foo` 同时以真实目录存在于 `~/.agents/skills/foo`（agents 分支 skip_symlink=false 扫入）和 `~/.claude/skills/foo`（claude 分支 skip_symlink=true 扫入真实目录）。主循环处理首个 foo（来自 agents）：adopt 把 agents/foo 迁池 → registry 落盘 → ensure_global_claude 建 agents 桥接（agents/foo 已迁空，OK）→ 建 claude 桥接时 `~/.claude/skills/foo` 是真实目录 → ensure_link 报 CanonicalCreate（symlink.rs:36-38）→ import 中断。第二个 foo（来自 claude）未及处理。
- 证据：
  - import.rs:34 agents scan `skip_symlink=false`（真实目录扫入）；import.rs:42 claude scan `skip_symlink=true`（真实目录扫入、symlink 跳过）。
  - symlink.rs:36-38 `else if link.exists() { return Err(CanonicalCreate) }`——claude 真实目录占位即报错（数据损失防护承重墙，不该放松）。
  - §6 同时存在两行：桥接占位行（「报 CanonicalCreate，import 中断」）与跨目录同名行（「dedup 只登记首个、其余成孤儿」）——对 agents+claude 同名真实目录场景，两行给出矛盾的结论（中断 vs 优雅 orphan）。
- 影响：用户有同名 skill 真实目录副本散落 agents+claude 时，跑 import 期望去重，实际拿到 CanonicalCreate 中断 + 引导手动处理。中断本身安全（数据不丢、引导清晰），但与 §6 承诺不符，且重跑会持续撞同一占位（registry 已落盘 canonical 指池、agents 桥接已建，但 claude 桥接每次失败）直到用户手动删 claude 副本。注：codex/cursor 副本不触发（桥接目标只有 agents/claude，不碰 codex/cursor，故 codex/cursor 同名副本确为优雅 orphan，符合 §6）。
- 修正建议：§6 跨目录同名行补一句限定——agents+claude 同名真实目录副本会触发 CanonicalCreate 中断（归入桥接占位行同等待遇），需用户先手动处理 claude 副本；codex/cursor 副本才走优雅 orphan。无需改设计（占位报错是承重墙，不能为便利放松），仅修 §6 措辞使两行不矛盾。
- 测试盲区：§8 无 agents+claude 同名真实目录的用例。建议补一条：预置 agents/foo + claude/foo（均真实目录）→ import 报 CanonicalCreate、池子不出现 foo、registry 不落盘 foo（或断言中断前的部分状态），锁定该失败模式。

### ⚪ P3 — §3.3 对「canonical_path 漂移到另一个真实目录」选择迁移而非跳过

- 现象：第 1 轮 P2 建议把「漂移到另一个真实目录」归入 dangling 同等待遇（跳过 + warn）。作者未采纳，§3.3 末段改为「canonical_path 指向一个存在的真实目录时（即使历史漂移）按当前指向迁移——relink 信任 canonical_path」。
- 评估：这是作者的有意决策，权衡声明清晰（信任 canonical_path 换取「canonical 被挪到 4 个扫描目录之外也能覆盖」的全面性）。canonical_path 由 skillkit 自己写（import/adopt/rescope），漂移到「错误的真实目录」需用户手编 registry.json，风险低。决策合理，不阻塞。仅提醒：若 canonical_path 因 skillkit bug 漂移到无关真实目录，relink 会用不可逆 rename 把该目录迁入池（源目录消失）——可在 §6 补一句不可逆风险声明，让用户知道 relink 对「存在的真实目录」是破坏性移动。

### ⚪ P3 — §3.4 提及 gemini，但 config.rs 默认只声明 claude-code/cursor/codex

- 现象：§3.4「池子 → ~/.agents/skills/<name>（agents 落地，cursor/codex/gemini 等直读，config.rs:26-44 默认声明 reads_agents_dir=true）」。config.rs:26-42 默认 agents vec 只有 claude-code（reads_agents_dir=false）、cursor（true）、codex（true），无 gemini。
- 影响：措辞性偏差，不影响逻辑（gemini 是举例，用户可按 config.toml 追加）。
- 建议：删去 gemini 或改成「cursor/codex 等直读（config.toml 声明的 reads_agents_dir=true 的 agent）」。

### ⚪ P3 — relink 的 registry save 时机未明确

- 现象：§3.2 明确了主循环「adopt → registry save → 桥接」的顺序。§3.3 relink 只说「adopt + 更新 canonical_path 指池」+「补桥接」，没说每 skill adopt 后是立即 save canonical_path 更新、还是遍历完批量 save。
- 影响：若批量 save，中途失败（如某个 skill adopt 撞 EXDEV）会丢已迁 skill 的 canonical_path 落盘（下次 relink 重复 adopt 那些已迁的——但 adopt 对「池有 src 空」幂等，最终一致，仅多一次空跑）。不大，但与 §3.2 的「先落盘再桥接」失败面分析风格不统一。
- 建议：§3.3 补一句「每 skill adopt 成功后立即 save canonical_path 更新（对齐 §3.2 顺序）」，让失败面可推导。

### ⚪ P3 — 计数器 relocated / unmanaged / imported 的重叠关系未说清

- 现象：§7 给 ImportReport 加 `relocated`（新发现迁池）+ `relinked`（存量补迁），保留 `unmanaged`。§4 summary「imported N（入池迁址 M，含存量补迁 K）」。但没说一个新发现的 unmanaged skill 是否同时进 `unmanaged` 和 `relocated`（它既是 unmanaged 源、又被迁池），以及 `imported` 是否含 relocated+relinked。
- 影响：实现者可能对计数口径理解不一（relocated 是否与 unmanaged 互斥）。
- 建议：§7 补一句维度说明——`unmanaged`/`reinstalled` 按 source 类型计（不变），`relocated`/`relinked` 按动作计（本次是否迁池），一个 skill 可同时出现在两个维度（如新发现 unmanaged 既在 unmanaged 又在 relocated）。

### ⚪ P3 — §3.2 步骤 0 需 `continue` 且对 dry_run 也要生效，spec 未点明结构重构

- 现象：步骤 0 列在步骤 5（dry_run）之前，暗示对 dry_run 也生效（dry_run 也应把 symlink canonical 计入 skipped 而非 unmanaged 预报）。但当前主循环结构是 `if pkg {} else if !dry_run {} else {}`（import.rs:71/89/104），dry_run 是独立分支。步骤 0 要在 dry_run 分叉前生效，需把 unmanaged 分支重构为先判 symlink、再分 dry_run。另外步骤 0 若只 push skipped 不 `continue`，会落到 import.rs:108 的 `imported.push` 双计。
- 影响：spec 意图清晰（symlink 跳过），仅实现结构需调整。
- 建议：§3.2 步骤 0 补「push skipped 后 continue（不落 imported），且判断在 dry_run 分叉前」。

## 3. 核对通过明细（供执行时对照，本轮逐项已验证）

| Spec 声明 | 验证结果（文件:行号） |
|---|---|
| §1 import.rs:89-108 unmanaged 只登记不迁文件不桥接 | 一致，import.rs:89 `} else if !dry_run {` … 108（registry 写在 90-101，无 adopt/桥接） |
| §3.2 import.rs:63-109 主循环 | 一致，import.rs:63 `for (name, canonical, package) in plan` … 109 |
| §3.2 import.rs:64-70 已登记 name 跳过 | 一致，import.rs:64-70（registered.contains → warn + skipped + continue） |
| §3.2 步骤 0 与 scan_dir agents 分支 skip_symlink=false 吻合 | 一致，import.rs:34 第二参数 `false`（skip_symlink）；import.rs:129-131 仅 skip_symlink=true 时跳 symlink → agents 分支 symlink-to-dir 确实扫入 plan，需步骤 0 拦截 |
| §3.2 步骤 3 对齐 install.rs:45-47 先落盘 registry | 一致，install.rs:45-47 `load`+`upsert`+`save` |
| §3.2 步骤 4 对齐 install.rs:50-52 install 后桥接 | 一致，install.rs:50-52 `if Global { ensure_global_claude }` |
| §3.2 步骤 5 import.rs:104-107 dry_run 分支 | 一致，import.rs:104 `} else {` … 107 |
| §3.1 复用 scope.rs:57-72 迁移+dedup | 一致，scope.rs:57-72（target.exists→remove src；else create parent+rename）；adopt 是正确提取 |
| §3.1 dedup 对齐 scope.rs:60-64 | 一致，scope.rs:60-64 |
| §3.1 裸 `?` 映射 Io（非 Tool）| 见 P2-2，error.rs:25-26 `Io(#[from]` |
| §3.3 scope.rs:45-56 不信任 canonical_path、扫物理位置 | 一致，scope.rs:44-56（`is_dir() && !is_symlink()` 过滤，agents_link/claude_link 找回） |
| §3.3 relink 与 scope.rs 判定差异权衡声明 | 准确，无自相矛盾（见 P3 漂移迁移决策评估） |
| §3.4 symlink.rs:10-22 ensure_global_claude 两层桥接 | 一致，symlink.rs:10-22 |
| §3.4 paths.rs:37-45 codex/cursor 历史目录注释 | 一致，paths.rs:37（codex 注释明示「import 扫描用；新设计下 agent 直读」）、:42-45（cursor） |
| §3.4 config.rs:26-44 reads_agents_dir=true | 一致，config.rs:35（cursor true）、:40（codex true）、:30（claude false）；gemini 不在默认（见 P3） |
| §3.5 EXDEV 兜底措辞弱化 | 一致，§3.5「通常同文件系统…跨文件系统时 EXDEV 按 §6 兜底」 |
| §4 cli import.rs:21-27 summary | 一致，cli import.rs:21-27（文案待按 §4 加 relocated/relinked） |
| §5 server skills.rs:340-357 import handler / :343-349 summary | 一致，skills.rs:340 `pub async fn import`、:343-349 summary format |
| §6 install.rs:61 uninstall 对 unmanaged 不删 canonical | 一致，install.rs:61 `if computed_hash.is_some()`；测试 install.rs:87-116 印证 |
| §6 symlink.rs:36-38 真实目录占位报 CanonicalCreate | 一致，symlink.rs:36-38 |
| §6 symlink-src 行（步骤 0 跳过）| 一致，与 §3.2 步骤 0 对齐 |
| §7 ImportRecord import.rs:11-21 加 relocated/relinked | 一致，import.rs:11-21（当前 4 字段，新增 2 字段非破坏） |
| §7 无新 error 变体 | 一致（CanonicalCreate/Io 均既有；Tool 命名见 P2-2） |
| §8 现有 4 测试 import.rs:196/244/258/277 | 一致，均指向 `#[test]` 行（fn 在下一行）；4 个测试名与描述对位 |
| §8 import_json_schema_locks_fields 对齐 install.rs:228 | 一致，install.rs:228 `install_local_json_schema_locks_fields`（MetaShape 子集序列化锁字段名）、rescope.rs:119 同类 |
| §8 集成测试 crates/core/tests/ | 目录已存在（m0/m1/m3_e2e.rs），「若缺则加」措辞略偏（不缺），无影响 |
| §10 README.md:94 import / :107 uninstall 描述 | 一致，README:94 import-existing 命令、:107「unmanaged skill 只删登记不删目录」 |
| §10 主 spec §459 历史目录迁移后归档 | 一致，§459「无法溯源的标记 unmanaged…历史私有目录不再作为落地目标，迁移后可归档」 |
| §12 ensure_link 对 dangling symlink 走 read_link 分支 | 一致，symlink.rs:29-31 read_link Ok→比对 target；dangling 的 exists 为 false 不进占位分支；§12 提醒「确认行为」方向正确（实际安全） |
| CLAUDE.md §5 canonical 单池子 / §6 --json 公开契约 / §8 schema 锁定 | 一致，CLAUDE.md:43（单版本）、:54（schema 契约）、:96（schema 锁定测试） |

## 4. 修正建议的执行顺序

1. 改 spec（`docs/superpowers/specs/2026-08-14-import-relocate-design.md`），均为文档层、不动设计：
   - **P2-1**（补桥接作用域）：§3.3 补桥接 bullet 把「无论 canonical 是否刚归池，都调」改为「若 canonical 已在池（含刚归槽）才调 ensure_global_claude」；§8 补 dangling 不建桥接的断言。这是唯一有有害后果（自指 symlink）的项，优先。
   - **P2-2**（错误变体）：§6 rename 行 `SkillkitError::Tool` → `Io`；§7「复用 CanonicalCreate / Tool」→「复用 CanonicalCreate；FS 错误走既有 Io」。
   - **P2-3**（跨目录同名）：§6 跨目录同名行补「agents+claude 同名真实目录副本触发 CanonicalCreate 中断（归入桥接占位），codex/cursor 副本才优雅 orphan」。
   - P3（gemini / relink save 时机 / 计数器维度 / 步骤 0 continue）：顺手补，不阻塞。
2. 改完 spec 再进入 writing-plans / 实现，按 §3 TDD 展开（现有 4 个 import.rs 测试更新 + §8 新增项，含 dangling 不建桥接、agents+claude 同名中断两条）。

## 5. 结论

第 1 轮 P1/P2/P3 全部落实且核对无误，本轮独立重审未发现 P0/P1 阻断项。3 个 P2 均为 spec 文档层问题（补桥接对 dangling 的作用域会产生自指 symlink、错误变体名 Tool/Io 不一致、跨目录同名中断与 §6 承诺矛盾），改 spec 措辞即可，不动核心设计。建议实现前消化这 3 个 P2（尤其 P2-1 有实际有害后果），P3 顺手补。设计整体扎实：行号声明零偏差，adopt/relink/顺序约束精准对齐既有 scope.rs/install.rs 模式，失败面与连带影响分析到位。
