# skill scope 转移与 profile 归属管理 — 设计 spec

> 日期：2026-08-04（2026-08-05 按 spec review 修订）
> 状态：待 writing-plans 落实现计划
> 关联：主 spec `docs/2026-07-29-skillkit-design.md` §5 / §8.4 / §9 / §10.1 / §11 / §12（本 spec 修订 §8.4 / §9 / §10.1，补充 §11 / §12）；`docs/design-decisions-2026-07-29.md` 决策 7；`docs/superpowers/specs/2026-08-01-skill-find-list-remove-design.md`（CLI find/list/remove，互补）；review `docs/review/2026-08-04-skill-scope-profile-design-spec-review.md`

## 1. 背景与动机

主 spec 设计了 skill 的 global/local scope（§8.3）和 profile 候选集（§8.4），但三个缺口：

- **scope 定死**：install 时定 scope（`crates/core/src/install.rs`），之后无法转移。skill 装成 local 后想全局化（或反之）只能卸载重装，丢版本锁和 profile/project 引用。
- **scope 与 profile 归属耦合混乱**：§8.4 允许 profile 引用 global skill（「apply 时幂等确保其全局存在」），但 global skill 是全局基座、进 profile 语义别扭；§10.1 又让 global 能进 `project.installed_skills`「声明依赖」，两层语义都不纯。
- **Skills 视图缺归属视角**：server Skills 视图（`crates/server/src/routes/skills.rs` + `templates/fragments/skills_main.html`）是 install/upgrade/remove 总览，没有 profile 归属展示、过滤、批量归入；用户管 profile 归属要跑到 Profiles 视图手填 `source/skill` 字符串（`profiles_main.html:10-13`，无 picker 无校验）。

本 spec 做三件事：
- scope 可双向转移（global↔local），转移即同步物理落地。
- 收紧语义：global skill 独立于 profile 和 project（core 硬约束）。
- Skills 视图改造为 scope + profile 归属管理中枢（chips 内联、高亮 toggle 批量归入、profile 过滤、scope 转移按钮）。

## 2. 语义变更：global 与 profile/project 归属互斥（core 硬约束）

### 2.1 新约束 + add_skill 校验

- global skill 不属于任何 profile（`profile.skills` 不得含 global skill）。
- global skill 不进任何 `project.installed_skills`。
- global = 纯全局基座：install / rescope 即全局生效，apply 完全不碰。
- local = 通过 profile 或 `project.installed_skills` 引用，apply 时落地到项目目录。

core 硬约束落点：`profile.add_skill`（`crates/core/src/profile.rs:37`）/ `project.add_skill`（`crates/core/src/project.rs:80`）加 scope 校验——目标 skill scope=global 则返回 `SkillkitError`，文案引导「global skill 不属于 profile/project，先 `skillkit rescope <id> local` 再归入」。

签名定死（scope 只存 registry，校验需访问）：`profile.add_skill(&mut self, id: &str, registry: &Registry)` / `project.add_skill(&mut self, id: &str, registry: &Registry)`。波及 CLI handler（`commands/profile.rs` 的 profile add-skill、project add-skill）+ 既有 `add_skill` 单测同步加 registry 参数；GUI/CLI 上下文已有 registry，成本低。scope 判定留 core（不在 handler 薄壳做，遵守 frontend-rules §1）。

两种错误必须区分（批量归入 §3.2 依赖）：
- `SkillAlreadyInstalled`（该 skill 已在，重复 add）——调用方可跳过或报错。
- scope=global（违反约束）——向上抛，引导 rescope。

### 2.2 skill→profile 反向索引

保持 profile→skill 单向持有（不在 `SkillMeta` 加 profile 归属字段，避免双向维护一致性、违反决策 7 的 DRY）。core 新增：

```rust
fn skill_profiles(paths, skill_id) -> Vec<String>   // 扫所有 profile 现算，返回含该 skill 的 profile name 列表
```

profile 数量小，现算不缓存（YAGNI，避免缓存失效）。global skill 永远返回空列表（不属任何 profile）。GUI 用它渲染「所属 profile」chips 列 + 驱动过滤。

渲染性能：GUI handler 一次性 load 所有 profile、内存遍历建 `skill_id → Vec<profile_name>` 反向 map（一次遍历 O(N×M)），模板从 map 取值，**不每行调 `skill_profiles` 现扫**（避免每行独立文件 IO）。

### 2.3 set_profiles 防绕过（堵 legacy 漏洞）

`Project::set_profiles`（`project.rs:110-123`）直接把所选 profile 的 skills 灌进 `installed_skills`，不经 `add_skill`，会绕过 §2.1 校验。改为：灌入时按 registry 查 scope，跳过 scope=global 的 id（防 legacy profile 含 global 进 installed_skills）。

签名定死：`set_profiles(&mut self, names: &[String], profiles: &[Profile], registry: &Registry)`。波及调用方（`projects::set_profiles` handler）+ 既有 `set_profiles_*` 测试同步加 registry 参数。

## 3. core 新增能力

### 3.1 set_scope(id, target_scope)

local→global：
1. 改 `registry` 的 scope=global
2. 建全局落地（复用 `crates/core/src/symlink.rs` 的 `ensure_global_claude`：`~/.agents/skills/<skill>` symlink + `~/.claude/skills/<skill>` 桥接）
3. 自动从所有 `profile.skills` 移除（§2.1 约束）
4. 自动从所有 `project.installed_skills` 移除（§2.1 约束）
5. 返回受影响项目列表（这些项目目录可能有 local 残留落地，提示重新 `project apply` 清理；`apply.rs` 现有 extra 清理逻辑收尾）

global→local：
1. 改 scope=local
2. 撤全局落地——**新增 `remove_global_claude(paths, meta)`**（`symlink.rs` 现只有建链函数，无移除）：
   - **不加 scope 守卫**：不镜像 `ensure_global_claude` 的 `meta.scope != Global → return Ok`（`symlink.rs:11-13`）——本函数在步骤 1 改 scope 之后调用，meta.scope 已是 local，加守卫会 no-op、留悬空链（registry 标 local、`~/.agents/skills/` 仍被 agent 直读）。调用方 set_scope 保证只在 global→local 调。
   - 删序：先 `~/.claude/skills/<name>`（→ agents_link）再 `~/.agents/skills/<name>`（→ canonical），反序会留悬空链。
   - 幂等：对不存在的 symlink 静默跳过（`std::fs::remove_file` 对缺失路径报错，重复 rescope 同方向零变化要求跳过）。
   - 真实目录守卫：**只删 symlink，不删真实目录**——`~/.agents/skills/<name>` 若是用户手工放置的真实目录（非 symlink），报错不删（数据损失防护，对齐 `ensure_link` 占位报错 `symlink.rs:36-39`）。
3. canonical 池子（`~/.skillkit/.agents/skills/<skill>`）保留。
4. 转完是「游离 local」（池子有、无引用），等用户归入 profile 或加进 project。

副作用时机：立即同步物理落地（跟 install/remove 一致，转移即生效），不延迟到 apply。

### 3.2 批量归入 profile（正确原子模式）

`profile.add_skill` / `project.add_skill` 本身**非幂等**（重复 add 返 `SkillAlreadyInstalled`，`profile.rs:37-43` / `project.rs:80-86`）。~~不靠 add_skill 幂等~~。批量正确模式：

- 不新增 core 批量函数（最小改动）。
- GUI 批量归入 = handler **load profile 一次 → 内存循环 `add_skill` → 一次 save**：
  - `SkillAlreadyInstalled` 静默跳过（该 skill 已在 profile，预期幂等意图）。
  - scope=global 错误向上抛，**不 save**（内存态丢弃，已处理的 add 不落盘）。
- 原子性来自「load 一次 + 内存循环 + 一次 save」，不是 add_skill 幂等。scope 错误时 profile 文件不变。

## 4. CLI

新增顶层命令（跟 find / list / remove 并列，`main.rs:21-43`）：

```
skillkit rescope <id> <global|local> [--yes] [--json]
```

两个方向都默认交互确认（都有影响，风险对齐——见 §5.4 风险分析）：
- local→global：「将移除 N 个 profile / M 个项目引用（**不可逆**，反转 rescope 只恢复全局落地、不恢复归属），确认？」
- global→local：「将撤销全局落地，N 个直读 `~/.agents/skills/` 的 agent 失去此 skill（可 `rescope global` 恢复），确认？」
- `--yes` 跳过；`--json` 隐含跳过（与 `remove` 的 `skip_confirm = yes || json` 模式一致，`skill.rs:119`）。

`--json` schema（公开契约，加 schema 锁定测试，CLAUDE.md §8）：

```
{ "id": String, "from": Scope, "to": Scope,
  "affected_profiles": [String], "affected_projects": [String] }
```

`Scope` serde lowercase（`registry.rs:8-13`），`from/to` 序列化为 `"global"/"local"`。

现有 `profile add-skill` / `project add-skill` 因 core 校验，传 global skill 自动报错引导，无需 CLI 侧改动。

## 5. GUI（Skills 视图改造）

### 5.1 布局（密集表格 A）

列：`id ｜ scope ｜ 所属 profile ｜ ops`

列变更去留（改 `skills_main.html`，现表头 `skills_main.html:23`）：
- 删 `source` 列（id 已含 `<source>/<skill>`，冗余）、`version` 列、`computed_hash` 列——Skills 视图聚焦 scope/profile 管理，版本信息非核心。
- 删每行 per-row install 表单（`skills_main.html:33-36`）：对已装 skill 重复 install 本就报 `SkillAlreadyInstalled`，冗余 UI；install 入口保留在顶部 find 流程（find 候选 → install-candidate）。
- `unmanaged` badge（`computed_hash.is_none()`，现 `skills_main.html:27`）保留，移挂到 id 列。
- 所属 profile：chips 内联，每个 chip 带 ×（需求 7）。
- ops：scope 转移按钮（local 行「→global」、global 行「→local」）+ upgrade + remove。

### 5.2 高亮 toggle + 批量归入 + 写操作端点

- 点 local 行切换选中态，选中时整行高亮（无 checkbox 列，直接操作）。global 行不可选（不属 profile，不能批量归入）。
- 有选中时顶部出现批量栏：「已选 N → [归入 profile ▾] [＋ 新建]」。归入下拉选现有 profile 批量加；＋新建输 name（只 name）建空 profile 后批量加。无选中不占位。

**新增端点（返回完整 Skills 页 `render_skills`，非片段——frontend-rules §1 写操作规约）：**

- `POST /{token}/skills/assign`：归入已有 profile。body `profile=<名>&id=<skill-id>`（id 重复 key，参照 `profiles.rs:139` 的 `form_urlencoded::parse` 收集多 id）。handler load profile 一次 → 内存循环 `add_skill`（`SkillAlreadyInstalled` 跳过、scope 错误抛错）→ 一次 save（§3.2）。
- `POST /{token}/skills/assign-new`：新建并归入。body `name=<新名>&id=<...>`。**先校验 profile 不存在**（存在则报错「profile X 已存在，改用归入或换名」，防 `profiles::create` 的覆盖语义 `profiles.rs:75-89` 清空已有 profile），再建空 profile → 循环 add_skill → save。
- `DELETE /{token}/skills/{id}/profile/{name}`：chip × 移除单个归属，返回完整 Skills 页。
- 路由注册追加到 `routes/mod.rs:34-44` skills 段。响应统一 `hx-target="body" hx-swap="outerHTML"`。

现有 `profiles::create`/`add_skill`/`remove_skill` 返回 Profiles 片段或完整 Profiles 页，**不复用**（会跳视图或塞片段进 body）。

**选中态 query 透传（修 review P1-2）：**

- 选中态存 URL query `?selected=id1,id2`。
- skills 页用**专属 query struct**（新建 `SkillsQuery { fragment, selected, profiles }`，不只用 `FragmentQuery`——后者只有 `fragment` 字段 `mod.rs:17-19`，serde 默认忽略未知字段，不扩就静默丢参）。
- 所有 skills 重渲染 handler（`render_skills` + 三个新写操作）从 query 读 `selected`（及 `profiles` 过滤 §5.3）透传给模板；模板按 `selected` 给行加高亮 class。
- 写操作 htmx 不 push-url，地址栏仍挂 `?selected=...`，服务端读 query 渲染高亮即不脱节。
- SSE 刷新（`layout.html:30-35` 现为 `htmx.ajax('GET', location.pathname + '?fragment=1', …)`）改为带上当前 query（保留 `selected`/`profiles` 再追加 `fragment=1`）；对其他无 query 状态的视图无害。

### 5.3 profile 过滤（定死：服务端 query param）

- 顶部 chips 多选（全部 / frontend / backend / …），OR 语义（属于任一选中 profile 的 local skill 都显示）。
- 机制：**服务端 query param `?profiles=fe,be`**。点 chip → `hx-get` 重渲 main（fragment 语义），与 `?selected=` 共存（`?selected=...&profiles=...&fragment=1`）。
- 「全部」= 不过滤、显示所有（含 global）；选具体 profile 时 global 不显示（`skill_profiles` 对 global 返回空）；回「全部」恢复。
- 不走客户端 JS：与选中态（URL）统一状态机制，所有 UI 状态在 URL + 服务端重渲染，SSE / 写操作重渲染无损——skillkit 已是 SSE 重渲染主模式（`compute_diff` 跳过 global、整页片段重渲），服务端过滤与此一致。

### 5.4 scope 转移的 GUI 确认 + 横幅

风险分析（修 review P2-2 倒挂）：
- local→global 移除该 skill 的**全部** profile/project 归属，**不可逆**（反转 global→local 只恢复全局落地，不恢复归属）——风险更高。
- global→local 只撤 symlink，**可逆**（`rescope global` 恢复）——风险更低。

GUI 两方向都直接执行 + 醒目横幅（去 hx-confirm 方向，commit b15d13e），但横幅明示风险：
- local→global：「✓ 已转全局，已从 N 个 profile / M 个项目移除引用；以下项目需重新 apply 清理目录残留：<list>」（移除动作不可撤销，需恢复归属用批量归入重新加回）
- global→local：「✓ 已转 local，撤销全局落地；N 个直读 `~/.agents/skills/` 的 agent 不再加载此 skill（可 rescope global 恢复）」

横幅内容与 CLI 输出对齐。GUI 不二次确认（去 hx-confirm 方向），靠横幅明示影响；不可逆性靠 CLI 确认 / GUI 横幅提示兜底，必要时用户 rescope 前看横幅。

### 5.5 Profiles 视图退化

Profiles 视图（`routes/profiles.rs` + `profiles_main.html`）退化为：创建 profile + 查看 profile 组成 + 拖拽排序。给 profile 加 skill 的手填 `source/skill` 表单（`profiles_main.html:10-13`）删掉（搬到 Skills 视图 §5.2 批量归入）。渲染 profile.skills 时惰性过滤 global（不显示 legacy global 引用，§8）。

### 5.6 现有保留

Skills 视图的 find 搜索、导入存量、全部升级保留。

## 6. 错误处理

| 场景 | 处理 |
|---|---|
| profile / project add global skill | 报错引导「先 `rescope <id> local` 再归入」（core 校验，CLI / GUI 统一） |
| 批量归入遇 `SkillAlreadyInstalled` | 静默跳过（该 skill 已在 profile） |
| 批量归入遇 scope=global | 向上抛错，不 save（§3.2 原子） |
| ＋新建 profile 已存在 | 报错「已存在，改用归入或换名」（防 `create` 覆盖清空） |
| rescope 建全局落地失败（权限 / symlink） | 原子回滚：scope 不改、registry 不落盘、已建 symlink 回滚 |
| rescope 撤全局遇真实目录（非 symlink） | `remove_global_claude` 报错不删（守卫，防数据损失） |
| rescope profile/project 移除阶段失败 | 非全量原子，给可恢复错误文案（列已改 / 未改清单），提示重试或手动 `remove-skill` |

**原子回滚范围声明**（修 review P2-4）：仅覆盖 scope 字段 + registry 落盘 + symlink 建/删。profile/project 引用移除是跨多文件的独立 `FileLock`（`lock.rs`）写，不在原子回滚内——该阶段失败时 scope/registry/symlink 已改、部分 profile/project 未改，给可恢复文案（列已改/未改），不声称全量原子。

## 7. 测试策略

core 单测（`crates/core/tests/`）：
- `profile.add_skill` / `project.add_skill` 拒绝 global skill（报错文案含 rescope 引导）；区分 `SkillAlreadyInstalled` 与 scope 错误。
- `set_scope` local→global：scope 改、全局落地建（`~/.agents/skills/` + `~/.claude/skills/` symlink 在位）、从 profile+project 移除、返回受影响项目。
- `set_scope` global→local：scope 改、撤全局落地（symlink 删）、canonical 池子保留。
- `remove_global_claude`：删序正确（claude 先 agents 后）、真实目录占位报错不删。
- `skill_profiles` 反向索引：多 profile 含同 skill、无 profile、global 永远空。
- `set_profiles` 跳过 global（legacy 防绕过）。

集成（tempdir 模拟 `~/.skillkit` + `~/.agents` + 项目目录）：
- local skill 转全局后，项目目录残留 → apply 清理。
- 幂等：重复 rescope 同方向零变化。
- `locked_shas` 残留：rescope 移除的 skill 的 sha 残留无害（`compute_diff` 只看 `installed_skills`，`apply.rs:323-328` 只 insert 不清理），测试注明。

HTTP 层（`crates/server/tests/routes.rs`，frontend-rules §5 片段契约）：
- skills 页渲染「所属 profile」chips 列 + unmanaged badge。
- `POST /skills/assign` 批量归入返回完整 Skills 页（非片段）；`SkillAlreadyInstalled` 跳过、scope 错误返错。
- `POST /skills/assign-new` 已存在报错（防覆盖）。
- `DELETE /skills/{id}/profile/{name}` 返回完整 Skills 页。
- `?selected=` 渲染高亮、`?profiles=` 过滤渲染（含 global 在「全部」显示、选具体 profile 不显示）。

GUI e2e（`make e2e`，playwright + chromium）：高亮 toggle 选中 / 取消、批量栏出现 / 消失；批量归入已有 / 新建 profile；过滤语义；chips × 移除；scope 转移按钮执行 + 横幅。

CLI：`rescope --json` schema 锁定；两方向确认三路径（默认问、`--yes`、`--json` 隐含跳过）。

验证：`make check`（单测 + clippy `-D warnings`）、`make e2e`（GUI）、`make run ARGS="rescope <id> global --json"` 手动走查。

## 8. 对主 spec 的修订点 + 存量限制

### §8.4 Profile
原文「profile 也允许引用 global skill（apply 时幂等确保其全局存在）」→ 改为「global skill 不属于任何 profile（core 硬约束：`profile.add_skill` 校验拒绝 global skill）。profile 只承载 local skill 的组合。」

### §9 分工表
更新：profile 只含 local skill；`project.installed_skills` 只含 local skill；global 不进二者。

### §10.1 apply 操作语义
原文「scope=global：install 时已全局落地，apply 只做幂等检查……进 installed_skills 是为了声明该项目依赖这个全局基座」→ 改为「scope=global：install / rescope 时即全局落地，apply 完全不碰 global（global 不进任何 `project.installed_skills`）。apply 只处理 scope=local 的 skill 落地。」

### §11 CLI
顶层命令区新增 `skillkit rescope <id> <global|local> [--yes] [--json]`。

### §12 GUI
Skills 视图描述扩展为「skill 管理中枢：scope 转移、profile 归属管理（chips / 过滤 / 批量归入 / 移除）、保留 find / install / upgrade / remove」，详见本 spec §5。

### 存量 global 引用策略（新增小节）

- 新约束（`add_skill` 校验 §2.1 + `set_profiles` 过滤 §2.3）防未来新增和 legacy 绕过 `installed_skills`。
- 存量 profile/project 里已有的 global 引用：**数据保留**（不静默删用户数据），GUI 渲染惰性过滤——`skill_profiles` 对 global 返回空（Skills 视图不显示归属）、Profiles 视图过滤显示、`set_profiles` 灌入跳过。
- **不做自动迁移工具**（YAGNI）；用户可手动 `profile remove-skill` / `project remove-skill` 清理 legacy global，或对 global skill `rescope local` 后正常归属。
- 已知限制：apply 对 legacy `installed_skills` 里的 global 幂等忽略（`compute_diff` 跳过 global，`apply.rs:37-38`），不报错、不落地。

决策推理追加到 `docs/design-decisions-2026-07-29.md`（决策 17：global 与 profile/project 归属互斥；决策 18：scope 转移副作用模型 + 风险对齐确认）。

## 9. 不做（YAGNI 边界）

- 不在 `SkillMeta` 加 profile 归属字段（保持单向，用现算反向索引）。
- 不给 `skill_profiles` 加缓存（profile 数量小）。
- 不新增 core 批量 `add_skills` 函数（handler load 一次 + 内存循环 + 一次 save 够，§3.2）。
- 不做 scope 转移的撤销栈（可逆靠反向 rescope，不做 undo 历史）。
- 不做存量 global 引用自动迁移工具（§8，数据保留 + 惰性过滤）。
- 不改 Sources / Projects 视图（本次只改 Skills + Profiles 退化）。
- `rescope` 命令暂放顶层，不为它单建 `skill` 子组（未来 skill 操作多了再归组）。
