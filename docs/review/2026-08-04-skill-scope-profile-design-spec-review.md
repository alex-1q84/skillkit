# Spec Review — skill scope 转移与 profile 归属管理设计（2026-08-04）

> 审查对象：`docs/superpowers/specs/2026-08-04-skill-scope-profile-design.md`
> 审查基准：对代码逐一核对（`crates/core` 的 profile/project/install/symlink/apply/registry/error、`crates/server` routes + templates、CLI main/commands、`docs/frontend-rules.md`、主 spec §8.4/§10.1）+ 需求 7 条逐项对照。
> 日期：2026-08-05
> 结论：**需求 7/7 全覆盖、语义收紧方向（global 与 profile/project 互斥）与主 spec 修订点核对属实**；但 §3.2 有一处与代码事实相反的事实性错误（add_skill 幂等），§5 有三处 GUI 写操作未落到具体端点/查询契约，须修正后再进 writing-plans。详见 §2。

## 1. 总体结论

需求覆盖对照（7/7 全对齐）：

| 需求 | spec 落点 | 评价 |
|---|---|---|
| 两种 scope（global/local） | 现有（`registry.rs:10` Scope）+ §2.1 收紧 | ✓ |
| global↔local 互相转移 | §3.1 `set_scope` + §4 CLI `rescope` + §5.4 GUI 按钮 | ✓ |
| 批量归入指定 profile（含即时新建） | §5.2 高亮 toggle + 批量栏 + 「＋新建」 | ✓（端点未定义，见 P1-1） |
| global 不属 profile；local→global 时从所属 profile 移除 | §2.1 硬约束 + §3.1 step 3 | ✓ |
| 按 profile 过滤 skill | §5.3 顶部 chips 多选 | ✓（机制未定，见 P1-3） |
| 列表列出所属 profile | §5.1 所属 profile chips 列 + §2.2 反向索引 | ✓ |
| 移除指定 profile 从属关系 | §5.1 chip × | ✓（端点未定义，见 P1-1） |

核心核对通过：

- **语义变更方向正确**：主 spec §8.4 line 236「profile 也允许引用 global skill」、§10.1 line 289「进 installed_skills 是为了声明该项目依赖这个全局基座」原文属实，§8 的修订点描述与原文对得上；「global 不碰 apply」与现状 `compute_diff` 跳过 global（`apply.rs:37-38`）一致。
- **§2.1 校验落点真实**：`profile.add_skill`（`profile.rs:37`）/ `project.add_skill`（`project.rs:80`）都存在，加 scope 校验可行。
- **§3.1 local→global 复用成立**：`ensure_global_claude`（`symlink.rs:10`）两层 symlink 幂等建，落点正确。
- **§5.2 引用的 SSE 逻辑属实**：`layout.html:30-35` 现为 `htmx.ajax('GET', location.pathname + '?fragment=1', …)`，带上当前 query 的改动成立，且对其它视图无害（其余视图无 URL query 状态）。
- **§5.1/§5.5 引用的模板落点属实**：`skills_main.html:23` 表头、`profiles_main.html:10-13` 手填 `source/skill` 表单。
- **§4 CLI 确认模式与现有 remove 一致**：`skill.rs:119` `skip_confirm = yes || json`，`rescope` 照抄即可。
- **`--json` schema 可锁定**：`Scope` serde lowercase（`registry.rs:8-13`），`from/to` 序列化为 `"global"/"local"` 字符串；`find_json_schema_locks_*` / `list_json_schema_locks_*`（`skill.rs:206-260`）已有先例。
- **§2.2 不在 SkillMeta 加归属字段**符合决策 7 DRY，现算反向索引成立（profile 数量小，YAGNI 合理）。

## 2. 必须修正 / 需决策的问题

### 🔴 P0 — §3.2「add_skill 幂等，循环不会部分失败」与代码事实相反

- `profile.add_skill` **不是幂等**：重复 add 返回 `SkillkitError::SkillAlreadyInstalled`（`profile.rs:37-43`），CLI `profile add-skill` 因此报错（`commands/profile.rs:39-44`）。
- GUI 批量归入 = 循环 load→`add_skill`→save（每轮独立落盘），中途某轮撞重复即**部分失败**：之前的 profile 已保存。spec 原文「add_skill 幂等，循环不会部分失败」两句话都不成立。
- §2.1 还要给 `add_skill` 加 scope 校验——批量循环会遇到两种错误：`SkillAlreadyInstalled`（该跳过）与「global skill 不可归入」（该报错）。spec 未区分。
- 修正（二选一，建议前者）：
  1. 批量循环对 `SkillAlreadyInstalled` 静默跳过、其余错误终止并回滚提示；§2.1 的 scope 错误照常向上抛。
  2. 把 `add_skill` 改为真幂等（重复静默），但注意这会改变 CLI `profile add-skill` 的既有报错语义，需在 spec 里声明这一行为变更。
- 无论哪种，spec 里的「幂等」「不会部分失败」表述必须删掉或改写为实际语义。

### 🟠 P1-1 — Skills 视图三个写操作的 HTTP 端点均未定义，现有 profiles 路由不可复用

- chip ×（需求 7）：现有 `DELETE /{token}/profiles/{name}/skills/{id}`（`routes/profiles.rs:117-135`）返回 `ProfileSkillsTpl` **局部片段**（`profile_skills.html`），是给 Profiles 视图局部替换用的。Skills 视图需要整表重渲染（完整页），直接复用会把 fragment 塞进 body（frontend-rules §1 禁止）。
- 批量归入 / ＋新建（需求 3）：现有 `POST /{token}/profiles/{name}/skills` 同样返回片段；`POST /{token}/profiles`（create）返回完整 Profiles 页，从 Skills 视图调用会跳视图。都没有适用于 Skills 视图的批量端点。
- 修正：spec 需为 Skills 视图明确新增端点，例如 `POST /{token}/skills/assign`（body：`profile=<已有名或新建名>&id=<...>` 重复 key，参照 `profiles.rs:139` 的 `form_urlencoded::parse` 收集）+ `DELETE /{token}/skills/{id}/profile/{name}`（返回完整 Skills 页，`render_skills` 系列），并把路由注册写进 spec（`routes/mod.rs:34-44` skills 段追加）。

### 🟠 P1-2 — 选中态恢复只覆盖 SSE 路径，写操作重渲染会丢高亮

- §5.2 说「重渲染后从 URL 恢复高亮」，但只描述了 SSE 刷新（layout.html）这一条路径。chip × / 批量归入 / rescope / upgrade / remove 等 POST/DELETE 全部返回 `render_skills(state, token, None, false)` 完整页（`routes/skills.rs:159,210,224` 现有模式），服务端渲染时不读 `?selected=`，高亮即丢——而 htmx 不 push-url，地址栏仍挂着 `?selected=...`，状态与页面脱节。
- 修正：所有 skills 页重渲染 handler（`render_skills` 及新写操作）统一从 query 读 `selected`（可能还有 `profiles` 过滤参数，见 P1-3）透传给模板；`FragmentQuery` 只有 `fragment` 字段（`routes/mod.rs:17-19`），需为 skills 页扩展 query struct（serde 默认忽略未知字段，不扩就静默丢参）。

### 🟠 P1-3 — §5.3 过滤机制未指定（服务端重渲染 vs 客户端 JS）

- spec 只写了 chips 多选 + OR 语义 + 数据源，没写点了 chip 之后怎么过滤：服务端 query param（`?profiles=fe,be`）重渲染，还是客户端 JS 隐藏行。
- 若走服务端：与 §5.2 的 `?selected=` 两个 query 参数要共存、过滤与选中态的交互（选中行被过滤掉是否保留选中）要交代；chips 点击用 `hx-get` 重渲 main 时需保持 fragment 语义。
- 若走客户端 JS：符合「不强制零 JS」边界（frontend-rules §1），无需 URL 状态、实现更轻，但要写清「选具体 profile 时 global 不显示」与「全部」往返的逻辑落点。
- 修正：spec 二选一定死并补一句交互细节，避免 writing-plans 阶段自由发挥。

## 3. 需决策 / 实现期注意（不阻塞方向）

### 🟡 P2-1 — global→local 撤全局落地需要新函数，spec 未列

`symlink.rs` 只有 `ensure_global_claude` / `ensure_link`（建），没有移除函数。§3.1 step 2「撤全局落地」需新增 `remove_global_claude`：
- 删除顺序：先 `~/.claude/skills/<name>`（指向 agents_link）再 `~/.agents/skills/<name>`（指向 canonical），反了会留悬空链。
- 安全语义对齐 `ensure_link`（`symlink.rs:25-39`）：**只删 symlink，不删真实目录**——`~/.agents/skills/<name>` 若是用户手工放置的真实目录，删掉是数据损失。spec 应写明这个新函数及其守卫。

### 🟡 P2-2 — local→global 的确认逻辑与数据风险倒挂

- §4/§5.4：global→local（只撤 symlink，`rescope global` 完全可逆）CLI 要交互确认；local→global（静默移除该 skill 的**全部** profile/project 归属，**不可逆**——反转只能恢复全局落地，不能恢复归属）反而 CLI/GUI 都直接执行。
- 数据损失方向判断反了。建议：local→global 至少输出「将移除 N 个 profile / M 个项目引用」并走确认（或 GUI 弹醒目二次确认），而不是只提示重新 apply。

### 🟡 P2-3 — 存量数据迁移缺位

- 现网语义下 profile 可含 global（§8.4）、project.installed_skills 可含 global（§10.1）。新约束只加在 `add_skill`（防未来），存量 global 引用不会被清：
  - `skill_profiles` 对 global 恒返回空（§2.2）与 profile.skills 实存矛盾，Profiles 视图仍会显示该 global skill；
  - `Project::set_profiles`（`project.rs:110-123`）把 profile.skills 直接灌进 installed_skills，legacy 会绕过 §2.1。
- 修正：spec 补一句迁移/惰性清理策略（如加载时过滤、rescope 触发清理，或明确「存量引用保留、apply 幂等忽略 global」的已知限制），不要求现在就做迁移工具。

### 🟡 P2-4 — §6「原子回滚」表述过强

- set_scope local→global 跨 registry + N 个 profile + M 个 project 多文件写（各有独立 FileLock，`lock.rs`），§3.1 的「落地失败原子回滚」只覆盖 scope 字段与 registry 不落盘；profile/project 移除阶段失败不在回滚范围，会留下部分移除的中间态。
- 修正：明确「原子回滚」范围 = scope 字段 + registry + symlink；profile/project 移除失败时给出可恢复的错误文案（列已改/未改清单），别声称全量原子。

### 🟡 P2-5 — 「＋新建」沿用 create 的覆盖语义，有清空风险

- `profiles::create`（`routes/profiles.rs:75-89`）无存在性检查，同名直接 save 覆盖（现网既有行为）。§5.2 批量「＋新建」若沿用，输入已有 profile 名会把该 profile 清空再塞新 skill。
- 修正：批量新建分支加存在性校验（存在则并入该 profile 或报错），并顺手把 create 本身的覆盖风险提一句。

## 4. 核对通过明细（供执行时对照，逐项已验证）

| Spec 声明 | 验证结果 |
|---|---|
| 主 spec §8.4 原文「profile 也允许引用 global skill（apply 时幂等确保其全局存在）」 | `docs/2026-07-29-skillkit-design.md:236` 属实，§8 修订点准确 |
| 主 spec §10.1 原文「进 installed_skills 是为了声明该项目依赖这个全局基座」 | `docs/2026-07-29-skillkit-design.md:289` 属实，§8 修订点准确 |
| 主 spec §11 顶层命令分组 | `main.rs:21-43`，新增顶层 `Rescope` 与 Install/Find/List/Remove/Upgrade 并列成立 |
| `profile.add_skill` / `project.add_skill` 存在 | `profile.rs:37` / `project.rs:80`，加 scope 校验落点正确 |
| `ensure_global_claude` 可复用 | `symlink.rs:10-22`，两层幂等建链；local scope 直接跳过 |
| `skill_profiles` 现算反向索引、不在 SkillMeta 加字段 | 符合决策 7（DRY）；profile 数量小、不缓存 YAGNI 合理 |
| Scope 序列化 lowercase | `registry.rs:8-13`，`--json` 的 from/to 为 `"global"/"local"` |
| global 不 per-project 落地、apply 幂等 | `compute_diff` 跳过 global（`apply.rs:37-38`）+ 测试 `diff_expected_only_local_global_skipped` |
| rescope 后项目残留由 apply「extra 清理」收尾 | `apply.rs:298-319` 现状支持，local→global 提示重新 apply 成立 |
| SSE `?fragment=1` 纯片段约定 | `layout.html:30-35` + frontend-rules §3；带上当前 query 改动安全 |
| `--json` 隐含跳过确认 | 与 `remove`（`skill.rs:119`）模式一致 |
| GUI 写操作返回完整页 | frontend-rules §1，Skills 新端点应沿用 `hx-target="body" hx-swap="outerHTML"` |
| 需求 3「即时创建新 profile」对齐 create（只 name） | `profiles::create` 只收 name，属实 |

## 5. 小项（不阻塞，实现期注意）

- **§5.1 列变更未显式说清去留**：新列 `id｜scope｜所属 profile｜ops` 隐含删掉现有 `source/version/computed_hash` 列（`skills_main.html:23`），且每行的 per-row install 表单（`skills_main.html:33-36`）也随之消失——对已装 skill 重复 install 本就报 `SkillAlreadyInstalled`，是冗余 UI，删合理，但 spec 应明说一句，并说明 `unmanaged` badge（`skills_main.html:27`）是否保留。
- **`skill_profiles` 每行现算 O(N×M) 文件 IO**：GUI 渲染建议 handler 一次性 load 所有 profile、内存建 `skill_id → Vec<profile>` 反向 map（一次遍历），别每行调 `skill_profiles` 现扫——不改 core 契约，只改调用方式。
- **rescope 后 `locked_shas` 残留**：apply 只 insert 不清理（`apply.rs:323-328`），被 rescope 移除的 skill 的 sha 残留无害（compute_diff 只看 installed_skills），计划里注明即可。
- **§7 测试清单补 HTTP 层**：除 core 单测 + e2e，建议按 frontend-rules §5 补 `crates/server/tests/routes.rs` 片段契约（skills 页渲染 chips 列、chip × 返回完整页、`?selected=`/`?profiles=` 渲染高亮与过滤）。
- **§5.4 确认语义两处统一**：GUI 直接执行 + 横幅反馈可以接受（去 hx-confirm 方向一致），但横幅要列出「已从 N 个 profile / M 个项目移除」，与 CLI 输出对齐。

## 6. 修正建议的执行顺序

1. **改 spec**（`docs/superpowers/specs/2026-08-04-skill-scope-profile-design.md`）再进 writing-plans：
   - §3.2：删掉「add_skill 幂等 / 不会部分失败」，改为「批量循环对 `SkillAlreadyInstalled` 跳过、scope 错误抛错」；§2.1 注明两种错误区分。（P0）
   - §5.1/§5.2：补 Skills 视图写操作端点（chip × / 批量归入 / ＋新建）与路由注册，响应统一完整页；补 `selected`（及过滤参数）在所有 skills 重渲染 handler 的 query 透传。（P1-1/P1-2）
   - §5.3：定死过滤机制（服务端 query param 重渲染或客户端 JS），补与 `?selected=` 的共存与交互。（P1-3）
   - §3.1：补 `remove_global_claude` 新函数（删序 + 只删 symlink 守卫）；§6 收窄「原子回滚」范围。（P2-1/P2-4）
   - §4/§5.4：local→global 补「移除 N profile / M 项目」的输出与确认权衡；§5.2「＋新建」补存在性校验。（P2-2/P2-5）
   - §8 或新小节：补存量 global 引用的一行迁移/限制说明。（P2-3）
2. 然后按交接流程产 writing-plans。

## 7. 结论（一轮）

Spec 值得进入执行：需求 7/7 覆盖、global 与 profile/project 互斥的语义收紧方向正确，主 spec 修订点（§8.4/§10.1）描述准确。但 **§3.2 的事实性错误必须先修**（add_skill 并非幂等，批量循环会部分失败），§5 的三个写操作要落到具体端点与 query 契约；其余 P2 均为方向性收尾，不改变设计决策本身。修正后即可执行。

## 8. 二轮 review（2026-08-05，spec 已修订）

审查对象：修订版 `docs/superpowers/specs/2026-08-04-skill-scope-profile-design.md`（头部标注「按 spec review 修订」）。一轮 10 项全部闭环，新引入/遗留 3 项 P2 实现期注意，详见下。

### 8.1 一轮问题闭环核对（10/10）

| 一轮问题 | 修订落点 | 判定 |
|---|---|---|
| P0 §3.2 add_skill 幂等错误 | §3.2 改为「load 一次 + 内存循环 add_skill + 一次 save」，明写非幂等、重复跳过、scope 错误不 save；§2.1 区分两种错误 | ✓ |
| P1-1 写操作无端点 | §5.2 新增 `POST /skills/assign`、`POST /skills/assign-new`（带存在性校验）、`DELETE /skills/{id}/profile/{name}`，统一返回完整 Skills 页，明写不复用 profiles 路由；`routes/mod.rs:34-44` 落点真实 | ✓ |
| P1-2 选中态 query 透传 | §5.2 新建 `SkillsQuery { fragment, selected, profiles }`，所有重渲染 handler 读 query 透传，SSE 带当前 query；SSE 改法对其他无 query 视图无害（其余视图 `location.search` 为空） | ✓ |
| P1-3 过滤机制未定 | §5.3 定死服务端 `?profiles=fe,be` + hx-get 重渲 main，与 `?selected=` 共存 | ✓ |
| P2-1 缺 remove_global_claude | §3.1 定义新函数（删序先 claude 后 agents、只删 symlink 不删真实目录，守卫对齐 `ensure_link`）| ✓ |
| P2-2 确认逻辑倒挂 | §4 两方向都默认交互确认 + 风险说明；§5.4 风险分析（local→global 不可逆、global→local 可逆），GUI 直接执行靠横幅，与去 hx-confirm 方向一致 | ✓ |
| P2-3 存量迁移缺位 | §8 新增「存量 global 引用策略」：数据保留 + GUI 惰性过滤 + `set_profiles` 跳过 + 不做自动迁移 + apply 幂等忽略 | ✓ |
| P2-4 原子回滚过强 | §6 声明回滚范围仅 scope+registry+symlink，profile/project 阶段失败给可恢复文案 | ✓ |
| P2-5 ＋新建覆盖语义 | §5.2 assign-new 先校验不存在，防 `create` 覆盖清空 | ✓ |
| 小项（列变更/unmanaged/反向 map/locked_shas/HTTP 测试） | §5.1 列去留 + unmanaged 挂 id 列；§2.2 渲染性能（handler 建反向 map）；§7 locked_shas 残留注明 + HTTP 层测试段 | ✓ |

新引用抽查通过：`apply.rs:37-38`（compute_diff 跳过 global）、`apply.rs:323-328`（locked_shas 只 insert）、`profiles.rs:139`（reorder 手动 parse）、`mod.rs:17-19`（FragmentQuery 只含 fragment）、`symlink.rs:36-39`（真实目录占位报错）、`skill.rs:119`（skip_confirm = yes||json）、`project.rs:110-123`（set_profiles）、`profile.rs:37-43`（add_skill 非幂等）。

### 8.2 二轮新发现（3 项 P2，不阻塞，建议顺手补进 spec）

#### 🟡 P2-A — `remove_global_claude` 的 scope 守卫陷阱（§3.1 global→local 顺序）

- §3.1 global→local 步骤 1「改 scope=local」**先于**步骤 2「撤全局落地」。若实现者镜像 `ensure_global_claude` 的守卫（`symlink.rs:11-13`：`meta.scope != Global → return Ok`）写 remove 函数，撤链时 meta.scope 已是 local → **直接 no-op，symlink 残留**（registry 已标 local、`~/.agents/skills/` 却仍被 agent 直读）。
- §7 测试「symlink 删」能兜住（非静默上线），但仍是实现期陷阱。修正：spec 明写一句「`remove_global_claude` 不加 scope 守卫（此时 meta.scope 已改），或把撤链移到改 scope 之前」。

#### 🟡 P2-B — scope 校验的 registry 来源未定义（§2.1/§2.3）

- `profile.add_skill`（`profile.rs:37`）/ `project.add_skill`（`project.rs:80`）现签名只有 `id`，无 registry 访问；scope 只存在 registry。§2.1「加 scope 校验」未说校验函数怎么拿到 scope。
- 同理 §2.3 `set_profiles`（`project.rs:110-123`）「按 registry 查 scope 跳过」也未定：core 内过滤需改签名（加 `&Registry` 参数）；若在 handler 过滤则把 scope 判定逻辑放进薄壳，违反 frontend-rules §1。
- 修正：spec 定死签名——`add_skill(&mut self, id, registry: &Registry)`（CLI `profile add-skill` `commands/profile.rs:39-44` 与既有测试同步改）、`set_profiles(&mut self, names, profiles, registry)`。GUI/CLI 上下文本就有 registry，成本低。

#### 🟡 P2-C — `remove_global_claude` 幂等性未声明

- §7「幂等：重复 rescope 同方向零变化」要求第二次 global→local 删链时**缺失链接应跳过**（`std::fs::remove_file` 对不存在路径会报错）。spec 只写了删序与真实目录守卫，未写缺失链接跳过。
- 修正：§3.1 或 §6 补一句「remove 对不存在的 symlink 静默跳过（幂等）」。

### 8.3 二轮结论

一轮 10 项全部闭环，修订质量高（P0 事实错误纠正到位、P1 契约补全、P2 方向收尾齐全），新引用抽查全部属实。剩余 3 项均为实现期注意项（P2-A 是顺序陷阱、P2-B 是签名契约、P2-C 是幂等声明），不影响设计方向。**可以进 writing-plans**；建议在 writing-plans 前把 8.2 三句补进 spec 对应小节（各一行），避免实现时踩坑。
