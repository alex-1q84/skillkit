# import 存量 skill 迁入 canonical 池设计

> 日期：2026-08-14
> 状态：review 收敛通过（3 轮：r1 修 P1×1 / P2×2 / P3×1，r2 修 P2×3 / P3×5，r3 修 P3×2；见 `docs/review/2026-08-14-import-relocate-design-spec-review{,-r2,-r3}.md`）；实现计划已就绪 `docs/superpowers/plans/2026-08-14-import-relocate.md`
> 关联：主 spec `docs/2026-07-29-skillkit-design.md` §459（历史私有目录迁移后归档，本 spec 实现该既定设计）；`CLAUDE.md` §5（canonical 单池子 / `~/.agents/skills/` 只放公共 skill 不挪用为暂存）；`docs/superpowers/specs/2026-08-07-install-local-skill-design.md`（入池 + 桥接模式参照）；`docs/superpowers/specs/2026-08-04-skill-scope-profile-design.md`（`scope.rs` 物理迁移 + dedup 逻辑参照）

## 1. 背景与目标

skillkit 有三条让 skill 进 registry 的路径：`install add`（npx 下载入池，managed）、`install local`（本地目录/zip 复制入池，managed）、`import-existing`（扫存量目录登记）。前两条都把文件物理放进 canonical 池 `~/.skillkit/.agents/skills/`，唯独 `import-existing` 对无法溯源（`.git` + remote）的 skill 走 unmanaged 登记：`import.rs:89-108` 只写 registry 一条记录（canonical_path 指向原位置、scope=Global、computed_hash=None），不移动文件、不建桥接 symlink。

三个后果：

- unmanaged global skill 物理上不在 canonical 池，与 §5「canonical 物理存储只有一份」的单池子模型冲突——同一份 skill 的真相散落在 `~/.agents/skills/`、`~/.claude/skills/`、`~/.codex/skills/`、`~/.cursor/skills/` 各处。
- 没有桥接 symlink（`ensure_global_claude` 只在 install / install_local / rescope 调用，import 的 unmanaged 分支从不调）：skill 若原本只在 `~/.claude/skills/<name>`，则直读 `~/.agents/skills/` 的 cursor/codex 发现不了；反之亦然，跨 agent 发现不一致。
- 与 managed global skill（install 来的）物理模型不统一，GUI 上同样标 global，落地路径却两套。

主 spec §459 早已写明设计意图：「无法溯源的标记 unmanaged……新设计下这些 agent 直读 `~/.agents/skills/`，历史私有目录不再作为落地目标，迁移后可归档。」本 spec 实现这一既定设计：让 import 的 unmanaged skill 也物理迁入 canonical 池，原位置用 symlink 取代，统一成 managed global skill 的物理模型（canonical 在池 + agents/claude 双层桥接）。

目标：

- import 时，新发现的 unmanaged skill 直接迁入池子 + 原位 symlink 桥接。
- import 时，幂等补迁 registry 里已登记但 canonical 仍在原位置的存量 unmanaged（一条 `skillkit import-existing` 把所有历史遗留统一归池）。

非目标（YAGNI）：不新增独立 `adopt` 命令（用户决策：改 import 默认，见 §9）；不改 uninstall 对 unmanaged 的行为（仍不删 canonical，§6）；不处理跨目录同名副本的自动合并（只迁 registry 记录的那一份）。

## 2. 边界（与现有路径对比）

| | install add | install local | import 可溯源（.git） | **import 无源（本次改）** |
|---|---|---|---|---|
| 来源 | npx（联网） | 外部目录/zip | 存量 + git remote | 存量，无源 |
| 动作 | 下载入池 | 复制/解压入池 | 重装入池（try_reinstall） | **迁入池 + 原位 symlink** |
| canonical | 池 | 池 | 池 | 池（改前：原位置） |
| 桥接 | global 时建 | global 时建 | global 时建 | **建**（改前：从不建） |
| computed_hash | 有 | 有 | 有 | None（不变，不可升级） |
| source | 真实 | `local` | 推导 | `unmanaged`（不变） |

改后 unmanaged 与 managed 的唯一差别只剩 source 名 + 有无 hash（能否升级），物理存储与桥接完全一致。

## 3. 核心设计

改 `crates/core/src/import.rs`。CLI/server 都是 `import_existing` 薄壳，逻辑层零改动（只 summary 文案，§4/§5）。

### 3.1 迁入函数 adopt_into_pool（私有）

新增私有函数，把真实目录 `src` 迁入池子。复用 `scope.rs:57-72` 已验证的迁移 + dedup 模式：

```rust
fn adopt_into_pool(paths: &Paths, name: &str, src: &Path) -> Result<PathBuf> {
    let target = paths.skillkit_skills_dir().join(name);
    if target.exists() {
        // 池子已有同名 canonical（旧 managed 残留 / 历史 rescope 迁移产物）：
        // canonical 以池子为权威。src 若仍存在（冗余副本）删之；src 已空（上次入池中断）跳过。
        if src.exists() {
            std::fs::remove_dir_all(src)?;
        }
    } else if src.exists() {
        // 池子空、src 在：迁移（同文件系统 rename 原子）
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(src, &target)?;
    }
    // 池子空且 src 也空（dangling）由调用方 relink 预检拦截，不进此处
    Ok(target)
}
```

adopt 保持纯迁移职责——`src` 必须是真实目录，调用方（主循环 §3.2 / relink §3.3）负责在调用前过滤 symlink（对齐 `import.rs:129` 的「只迁真实目录」原则）。不在此处判断 src 类型，避免职责混杂。

dedup 语义对齐 `scope.rs:60-64`（global→local 迁移遇池子已有同名：删全局副本、池子权威）。`src.exists()` 前置检查让 adopt 对「上次 adopt 已入池、registry 未落盘」的中间态幂等（重跑时 target 在、src 空 → 直接返回 target，不报错）。

### 3.2 新发现 unmanaged：主循环分支改造

`import_existing` 主循环（`import.rs:63-109`）的 unmanaged 分支（无 package），从「只登记」改为「迁入 + 登记 + 桥接」：

0. 若 `canonical` 是 symlink（`~/.agents/skills/` 分支 `skip_symlink=false` 会扫进 symlink-to-dir），`report.skipped.push` 后 `continue`（不落 `imported`，防双计）——`rename` 一个 symlink 只移动链接本身、池中 canonical 变成指向原目标的悬空 symlink，真实内容未入池，破坏「canonical 是真实目录」模型。此判断须在 dry_run 分叉**前**生效（dry_run 也把 symlink canonical 计入 skipped 而非 unmanaged 预报，需把现有 `if pkg {} else if !dry_run {} else {}` 结构 `import.rs:71/89/104` 重构为先判 symlink、再分 dry_run）。对齐 `import.rs:129` 与 §3.3 relink 的「只迁真实目录」。
1. `adopt_into_pool(paths, &name, Path::new(&canonical))` 把扫描到的真实目录迁入池子，得池子 canonical。
2. 构造 meta：canonical_path 指向池子（不再指原位置），source=unmanaged、scope=Global、computed_hash=None 不变。
3. `Registry::upsert` + `save`（对齐 `install.rs:45-47` 的顺序：先落盘 registry 再桥接）。
4. `ensure_global_claude(paths, &meta)` 建桥接——与 `install.rs:50-52` install 后桥接对称。
5. dry_run（`import.rs:104-107`）只统计（`report.relocated`），不迁文件、不桥接。

顺序要点（adopt → registry save → 桥接）：adopt 成功后先落盘 registry（canonical 指池），再桥接。即便桥接失败，registry 已一致（canonical 指池），relink 下次按 §3.3 补建桥接即可收敛，不留「canonical 指空位置」的坏记录。

### 3.3 历史存量补迁：relink_unmanaged

只改主循环不够：import 是幂等的（`import.rs:64-70` 已登记的 name 跳过），改默认行为不会触及已登记的存量 unmanaged——它们 canonical 仍散落在原位置。需补一步。

新增 `relink_unmanaged(paths, report, dry_run)`，在 `import_existing` 开头（主循环前）跑。遍历 registry，对 `source=="unmanaged"` 且 `scope==Global` 的 skill 做两件事：

- **归池**：若 `canonical_path` 不在池子（`!starts_with(skillkit_skills_dir())`）：
  - canonical 不存在（dangling，用户手动删过原位置）→ `tracing::warn` 跳过，不中断。
  - canonical 是 symlink（历史部分桥接残留）→ 跳过（只迁真实目录，对齐 `import.rs:129` skip_symlink 约定）。
  - canonical 是真实目录 → `adopt_into_pool` + 更新 canonical_path 指池，计入 `report.relinked`。
- **补桥接**：仅当 canonical 已在池（`canonical_path.starts_with(skillkit_skills_dir())`，含刚归槽成功与本就在池）时调 `ensure_global_claude`（幂等：`symlink.rs:29-31` 指向正确即跳过）。这覆盖「上次 adopt 成功 + registry 落盘 + 桥接中途失败」的中间态——canonical 已在池、桥接缺失时，relink 不再 adopt（canonical 已在池跳过）但补建桥接，彻底收敛。**不**对归槽跳过的 dangling/symlink canonical 补桥接——它们的 canonical_path 仍指原位（agents/claude 路径之一），补桥接会拿该路径既当 target 又当 link 建出自指/互指环（如 `~/.agents/skills/<name> → ~/.agents/skills/<name>`），让 cursor/codex 扫描拿 ELOOP。归槽 warn 跳过与补桥接跳过在此对齐。

为何扫 registry 而非复用主循环：主循环只扫 4 个固定目录发现「新」skill，对「已登记」的 name 直接跳过；补迁要处理的是 registry 里 canonical 已漂移的记录，按 registry 遍历更全面（canonical 可能已被用户挪到 4 目录之外）。

save 时机：每 skill 归槽成功（canonical_path 更新指池）后立即 save registry（对齐 §3.2 的「adopt → registry save → 桥接」顺序），让失败面可推导——中途某个 skill 撞 EXDEV 中断时，已迁的 canonical_path 已落盘，下次 relink 对「池有 src 空」幂等空跑，最终一致。

dry_run 时 relink 只统计（`report.relinked` 预报），不迁移、不桥接，与主循环 dry_run 语义一致。

与 scope.rs 的判定差异（权衡声明）：`scope.rs:45-56` 不信任 `registry.canonical_path`（可能漂移），global→local 时扫物理位置（agents_link / claude_link）找回真实 canonical。relink 有意不同——以 `registry.canonical_path` 为遍历源，换取「canonical 被挪到 4 个扫描目录之外也能覆盖」的全面性。代价：`canonical_path` 漂移到不存在路径时只能 warn 跳过（不具备 scope.rs 的物理位置找回）；`canonical_path` 指向一个存在的真实目录时（即使历史漂移）按当前指向迁移——relink 信任 `canonical_path`，这正是它与 scope.rs 的有意差异。漂移 orphan 的手工清理已在 §6 声明。

### 3.4 桥接模型（复用，不改 symlink.rs）

迁入池子后调 `ensure_global_claude`（`symlink.rs:10-22`），建两层 symlink：

- `池子 → ~/.agents/skills/<name>`（agents 落地，config.toml 声明 `reads_agents_dir=true` 的 agent 直读，默认 cursor/codex，`config.rs:26-44`）
- `~/.agents/skills/<name> → ~/.claude/skills/<name>`（Claude 桥接）

`~/.codex/skills/`、`~/.cursor/skills/` 是历史私有目录（`paths.rs:37-45` 注释明示「import 扫描用；新设计下 agent 直读 `~/.agents/skills/`」），迁空后这两个 agent 靠 `~/.agents/skills/` 桥接发现，符合主 spec §459「迁移后可归档」。

### 3.5 原子性与并发

- rename 原子：src 与 target 都在 `$HOME` 下（生产）或同一 tempdir（测试），通常同文件系统（除非用户把 `~/.skillkit` 跨卷 mount），`std::fs::rename` 原子；跨文件系统时返回 `EXDEV`，按 §6 兜底报 `Io` 错（`#[from]` 映射），canonical 不动。
- 失败面：adopt（rename）失败 → canonical 未动、registry 不落盘（分支内先 adopt 成功才 upsert+save），不留半迁。桥接失败（ensure_global_claude）→ 文件已入池、registry 已落盘（canonical 指池），下次 import 的 relink 补建桥接收敛（§3.3）。
- 并发：import_existing 现有流程未持 `FileLock`（既有债，与 install_local spec §3.7 同源）。本次不引入锁（YAGNI，import 是低频人工操作），记为后续工作。

## 4. CLI（薄壳，文案）

`crates/cli/src/commands/import.rs:21-27` summary 增计入池计数，与 `--json` 的 `ImportReport` 字段对齐（§7）：

```
imported N（入池迁址 M，含存量补迁 K），reinstalled ...，skipped ...
```

- M = 新发现并迁池的数量（`report.relocated`）。
- K = 存量补迁的数量（`report.relinked`）。

命令表面、参数、`--dry-run` / `--json` 全不变。最终措辞实现时定。

## 5. GUI（import handler 文案）

`crates/server/src/routes/skills.rs:340-357` import handler summary（`:343-349`）增 relocated / relinked 计数，与 CLI 对齐。handler 仍是 `import_existing` 薄壳，无新端点、无模板改动。「导入存量 skill」按钮（`skills_main.html`）交互不变。

## 6. 错误处理（反馈引导行动）

| 场景 | 处理 |
|---|---|
| adopt 时池子已有同名 canonical | 删原位置冗余副本（若存在），池子权威（对齐 `scope.rs:60-64`，不报错） |
| relink 遇 canonical 不存在（dangling） | warn 跳过，不中断（用户手动删过，registry 记录仍在，可后续手工清） |
| relink 遇 canonical 是 symlink | 跳过（只迁真实目录） |
| 桥接遇 `~/.agents/skills/<name>` 或 `~/.claude/skills/<name>` 是真实目录占位 | `ensure_global_claude` → `ensure_link` 报 `CanonicalCreate`（`symlink.rs:36-38`），不静默删；import 中断，报错引导用户先手动处理占位目录 |
| rename / FS 失败（权限 / 跨文件系统 EXDEV） | `SkillkitError::Io`（`error.rs:25-26` `#[from]`，adopt 裸 `?` 自动映射），canonical 未动、registry 不落盘；保留原始 io 信息（如 EXDEV「Cross-device link」） |
| 主循环扫到 `~/.agents/skills/<name>` 是 symlink（agents 分支 `skip_symlink=false`） | skipped，不 adopt（`rename` symlink 只移链接、池中 canonical 变悬空 symlink 破坏模型），对齐 `import.rs:129`「只迁真实目录」 |
| 跨目录同名副本（同名 skill 散落多个目录） | import dedup 只登记首个（`import.rs:64-70`），relink 只迁 registry 记录的 canonical 那份。codex/cursor 副本留原地成优雅 orphan（桥接不碰这两个目录）；但 agents+claude 同时有同名真实目录副本时，首个 adopt 入池后建 claude 桥接会撞 claude 真实目录占位 → `CanonicalCreate` 中断（归入上面桥接占位行同等待遇），需用户先手动删 claude 副本，重跑会持续撞同一占位直到清理 |

uninstall 连带影响（行为不变，仅位置变）：unmanaged 的 `computed_hash=None`，`uninstall`（`install.rs:61`）本就不删 canonical。改后 canonical 在池子，uninstall 仍只摘 registry 记录 → 池子留孤儿目录 + 桥接 symlink 变 dangling。下次 `import-existing` 的 relink 不会重新登记孤儿（canonical 已在池，跳过归池），需用户手动 `rm` 池子目录 + 残留 symlink，或在 GUI remove。本 spec 不改 uninstall 范围（YAGNI），仅声明该连带影响。

relink 破坏性移动风险：relink 对「canonical_path 指向一个存在的真实目录」是破坏性 rename（不可逆，源目录消失）——这是信任 canonical_path 的代价（§3.3）。canonical_path 由 skillkit 自管（import / adopt / rescope 写入），正常不指向无关真实目录；但若因 skillkit bug 漂移到无关目录，relink 会误迁该目录入池。风险低，本 spec 不额外防护（YAGNI），仅声明。

## 7. 组件与依赖

- `crates/core/src/import.rs`：
  - 新增私有 `adopt_into_pool(paths, name, src) -> Result<PathBuf>`（§3.1）。
  - 新增 `relink_unmanaged(paths, report, dry_run) -> Result<()>`（§3.3），`import_existing` 开头调用。
  - 改主循环 unmanaged 分支（`import.rs:89-108`）：adopt → registry upsert+save → `ensure_global_claude`。
  - `ImportReport`（`import.rs:11-21`）加 `relocated: Vec<String>`（新发现迁池）+ `relinked: Vec<String>`（存量补迁），`#[derive(Serialize)]`——`--json` schema 扩展，新增字段不破坏既有消费者（CLAUDE.md §6 schema 契约，新增非破坏）。计数维度：`unmanaged`/`reinstalled` 按 source 类型计（不变），`relocated`/`relinked` 按动作计（是否本次迁池）；一个 skill 可同时出现在两个维度（如新发现 unmanaged 既在 `unmanaged` 又在 `relocated`），`imported` 含全部登记成功的。
- 无新依赖、无新模块、无新 error 变体（复用 `CanonicalCreate`；FS 错误走既有 `Io`（`error.rs:25-26` `#[from]`，adopt 裸 `?` 自动映射，对齐 `scope.rs:64/69` 范式源））。
- CLI / server：仅 summary 文案。

## 8. 测试策略

core 单元（`import.rs` tests，现有 4 个需更新）：

- `import_registers_unmanaged_and_skips_invalid`（`import.rs:196`）：canonical 断言从「原位置」改为「池子 `skillkit_skills_dir().join(name)`」；新增断言 `~/.agents/skills/<name>`、`~/.claude/skills/<name>` 为 symlink（桥接在位）；codex/bar 迁池后 `~/.codex/skills/bar` 不再是真实目录。
- `import_dry_run_writes_nothing`（`import.rs:244`）：dry_run 不迁文件（原位置仍是真实目录）、不建桥接、registry 空。
- `import_dry_run_dedups_same_name_across_dirs`（`import.rs:258`）：dry_run 预报语义不变。
- `import_is_idempotent`（`import.rs:277`）：二次跑 relink 发现已入池 + 桥接在位 → 跳过，主循环发现已登记 → 跳过，零变化。

新增：

- adopt 池子已有同名 dedup：预置池子 `<name>` + 原位置 `<name>` → import 后池子保留、原位置副本删除、canonical 指池子。
- relink 存量补迁：registry 预置 canonical 在 `~/.agents/skills/<x>` 的 unmanaged（global）→ import 后迁池 + 桥接 + canonical 更新 + `report.relinked` 含 x。
- relink 补桥接（中间态收敛）：registry 预置 canonical 已在池、但 `~/.agents/skills/<x>` 桥接缺失 → import 后 relink 不重 adopt、补建桥接 symlink。
- relink 边界：canonical dangling（warn 跳过，且**不**补建桥接——断言 `~/.agents/skills/<x>`、`~/.claude/skills/<x>` 均无新建 symlink，验无自指/悬空环，对应 P2-1）、canonical 是 symlink（跳过，同样断言无新建 symlink）、canonical 已在池且桥接在位（全跳过）。
- 桥接占位报错：预置 `~/.claude/skills/<name>` 真实目录占位 → import 中断报 `CanonicalCreate`，池子 / registry 不变。
- symlink-src 跳过（验 P1）：预置 `~/.agents/skills/<name>` 为指向外部真实目录的 symlink → import 后该条进 skipped，池子不出现同名 symlink-canonical、原 symlink 保留。
- 跨目录同名中断（验 P2-3）：预置 agents/foo + claude/foo（均真实目录）→ import 报 `CanonicalCreate`（agents/foo adopt 入池后建 claude 桥接撞占位）；codex/cursor 同名副本则走优雅 orphan（不报错、池子不出现）。

集成（`crates/core/tests/`，若缺则加）：tempdir 全流程——预置 4 目录存量 skill → import → 断言全部 canonical 在池 + agents/claude 双层 symlink + codex/cursor 历史目录已迁空。

CLI：新增 `import_json_schema_locks_fields`（对齐 install.rs 的 `install_local_json_schema_locks_fields` 写法，CLAUDE.md:96），断言 `--json` 输出含 `imported / unmanaged / reinstalled / skipped / relocated / relinked` 字段名——import 命令此前缺 schema 锁定测试，本次顺带补齐。

GUI e2e（可选，`make e2e`）：「导入存量」按钮执行后 summary 含入池计数。

验证：`make check`（单测 + clippy `-D warnings`）+ `make run ARGS="import-existing --dry-run"` 手动走查预览。

## 9. 关键决策与否定备选

- 改 import 默认行为（用户决策），不新增独立 adopt 命令：一条 `import-existing` 统一归池，符合「把所有导入的 skill 纳入管理」意图。否定：独立 `skill adopt [--all|<id>]` 命令 + GUI 按钮（import 保持只登记）——更可控但多一步，且存量仍需批量入口，不如直接收敛进 import。
- 历史存量靠 relink 自动补迁（非用户手动）：import 幂等跳过已登记，光改主循环触及不到存量；relink 按 registry 遍历补迁，一次 import 全归池。否定：要求用户先删 registry 记录再重 import（破坏性、易丢元数据）。
- 迁移用 rename（同文件系统原子），不做 copy + delete：src 与 target 同在 `$HOME`，rename 原子且零拷贝。否定：copy 到暂存再 rename（install_local spec §3.2 三段模式）——那是跨源复制不可信输入的场景，import 迁的是已在用户目录的可信文件，无需暂存。
- 池子已有同名时删原位置副本（池子权威），对齐 scope.rs：不引入 force 开关。否定：报错让用户选——原位置副本本就是冗余（import 登记的目的就是归池），报错增加无谓摩擦。
- 不引入 force 跳过桥接占位：`ensure_link` 真实目录占位报错是数据损失防护承重墙（`symlink.rs:36-38`、scope spec §3.1），不为便利放松。占位罕见（用户手工放了同名真实目录），报错引导手动处理足够。
- relink 对池内 canonical 幂等补桥接（不只处理归槽，也补缺失桥接）：覆盖「adopt 成功 + 桥接中途失败」的中间态，让重跑 import 彻底收敛；dangling/symlink 归槽跳过的不补桥接（防自指环，§3.3）。否定：relink 只管归池、不管桥接——会留「canonical 在池但桥接缺」的未收敛态，agent 发现不了。

## 10. 对主 spec 的呼应

主 spec §459 已写明「历史私有目录不再作为落地目标，迁移后可归档」，本 spec 是该设计的实现落地。unmanaged 的语义定义（虚拟源、computed_hash=None、scope=global、不可升级）不变，仅物理位置从「原位置登记」统一为「池子 + 桥接」，与 managed global skill 物理模型对齐。无需改主 spec 正文（§459 描述已涵盖），仅需在实现后于 README（`README.md:94` import 命令描述、`:107` uninstall 描述）同步措辞。

## 11. 不做（YAGNI 边界）

- 不新增独立 adopt 命令 / GUI 按钮（§9）。
- 不改 uninstall 对 unmanaged 的行为（仍不删 canonical，§6 连带影响声明）。
- 不处理跨目录同名副本自动合并（只迁 registry 记录那份，孤儿留待手动清）。
- 不引入 force 跳过桥接占位（§9）。
- 不给 import_existing 加 FileLock（§3.5 既有债，低频人工操作）。
- 不改 codex/cursor 历史目录的扫描（import 仍扫，迁空后下次扫到空目录自然跳过）。

## 12. 后续提醒

- `ImportReport` 加字段是 `--json` schema 扩展，虽新增不破坏，但 AI agent 可能依赖该 schema；实现后跑一次 `--json` 确认输出结构。
- unmanaged 迁池后，GUI 上 unmanaged 与 managed global skill 视觉无差（都 global），仅 source / hash 列区分——符合统一管理意图；用户可能困惑「为什么这个 global skill 不能 upgrade」（无 hash），靠 unmanaged badge（`computed_hash.is_none()`）区分，该 badge 保留。
- relink 补桥接遇 dangling 桥接（uninstall 后留的悬空 symlink）时，`ensure_link`（`symlink.rs:29-35`）会删旧悬空链重建——实现时确认 dangling symlink 在 `read_link` / `exists` 分支的行为，避免误判占位报错。
