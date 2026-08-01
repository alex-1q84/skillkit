# Spec Review — GUI 对齐 CLI 全功能设计（2026-08-01）

> 审查对象：`docs/superpowers/specs/2026-08-01-gui-parity-design.md`
> 审查基准：对代码逐一核对（`crates/core`、`crates/cli`、`crates/server`、templates、tests）+ 交接记录 `docs/sessions/2026-07-29-skillkit-design.md` §3.3。
> 日期：2026-08-01
> 结论：**spec 可执行，但执行 plan 前须修 1 个计划级 bug（Task 9 模板 target），Task 5 的 upgrade_all 参数需现场决策**。详见 §2。

## 1. 总体结论

Spec 质量高、可执行：

- **8 条缺口盘点真实**：`routes/mod.rs` 现有路由仅 10 条操作，8 条缺口与 spec §1 差距表逐一对应。
- **7 条「core 已具备」声明全部核对通过**：`npx::find` / `install` / `import_existing` / `upgrade_all` / `Project::register` / `proj.rebind` / `proj.apply_profile`+`Profile::load`+`list_profile_names` 均存在且签名与 spec 调用一致。
- **`scan_projects` 确在 cli 层私有**（`crates/cli/src/commands/project.rs:154-171`），下沉方案正确，Task 1 的 lib.rs re-export 修改行与现状逐字一致。
- **架构决策合理**：同步 find + `hx-indicator`（YAGNI）、GUI 端点不加 `--json`（职责分离）、handler 薄壳调 core，均符合 CLAUDE.md §7。
- **fake_npx 测试基建参数对位**：计划假脚本的 `find $2` / `add $5=skill` / `update $3=skill` 与 `crates/core/src/npx.rs` 真实命令完全一致；范式源自 `upgrade.rs` 的 `install_fake_npx`。

## 2. 必须修正 / 需决策的问题

### 🔴 P0 — Task 9 apply-profile 表单 `hx-target="body"` 是计划级 bug（spec §3.4 未声明返回类型，plan 自相矛盾）

- handler（plan Task 9 Step 4）返回 `status_fragment(state, proj)` → 只渲染 `fragments/status.html` = `#status-panel` div。
- 模板（plan Task 9 Step 5）却写 `hx-target="body" hx-swap="outerHTML"` → **点击「应用 profile」会把整个页面替换成一行 status 面板，工作台全部消失**。
- 正确做法：对齐 `workspace_main.html:8,14` 既有 set_skills/apply 表单模式，改为 `hx-target="#status-panel" hx-swap="outerHTML"`。
- 测试盲区：现有测试只断言 project.toml 落地（`applied_profiles`/`installed_skills`），不查响应体，**测试会绿但 UI 是坏的**——建议补一个断言响应含 `status-panel` 的用例。

### 🟠 P1 — Spec §3.2 `upgrade_all(paths, true)` 与「blocked 不静默列出」矛盾，blocked 恒为空

- `crates/core/src/upgrade.rs:29`：`if !affected.is_empty() && !yes { return Err(UpgradeBlocked) }` —— **`yes=true` 时 `upgrade_skill` 永不返回 UpgradeBlocked**，`upgrade_all(..., true)` 的 `blocked` 永远是 `[]`。
- 后果：plan Task 5 summary 里「；跳过 {}（影响项目…）」分支是死代码；更糟的是冲突 skill 被**静默自动升级**，锁定其 hash 的项目基线漂移而零反馈——违背主人「列出不拦截」决策（CLI `--all` 默认 `yes=false` 才触发列出）。
- 建议：GUI 改调 `upgrade_all(&state.paths, false)`，blocked 照常列出（与 CLI `--all` 默认语义对齐，blocked 只是列出不升级，无二次确认）。若坚持 `yes=true`，summary 应改从 `all.upgraded[].affected_projects` 汇总，而非 `all.blocked`。
- 测试盲区：Task 5 测试没种「锁了 oldhash 的 project」，无法暴露此问题。建议加一个带锁定项目的用例验证「冲突列出 / 不静默」。

### 🟡 P2 — find/scan 片段 `hx-swap="innerHTML"` + 片段外层 `id` div → 嵌套重复 id

- plan Task 2 Step 6：输入框 `hx-target="#find-results" hx-swap="innerHTML"`，而 `find_results.html` 外层是 `<div id="find-results">` → 替换结果是 `<div id="find-results"><div id="find-results">…</div></div>`，重复 id。
- Task 7 的 scan 同样问题（`#scan-results`）。
- §7.5 的「片段外层固定 id」正是为 `outerHTML` 替换设计的。修正二选一：
  1. 改用 `hx-swap="outerHTML"`（保留外层 id，最贴合 §7.5）；
  2. 片段去掉外层 wrapper div、纯表格内容配 innerHTML。

### 🟡 P2 — Spec §4 技术陈述小误（结论不变）

- 「Axum 异步 runtime 下同步阻塞只占一个 blocking 线程（tokio 默认 512）」不准确：`Command::output()` 在 async 里直接调用**阻塞的是 tokio 工作线程**（默认 = CPU 核数），不是 512 的 blocking 池；`spawn_blocking` 才走那个池。
- 对本项目结论无影响（单用户本地 GUI，2-5s 阻塞可接受，YAGNI 成立），但建议顺手用 `tokio::task::spawn_blocking` 包 find，写法更严谨，风险零。

### ⚪ P3 — 小项（不阻塞）

- Task 6 测试的 `urlencode` helper 只编码 `/`——CI 临时路径够用，但换路径含空格/`&` 会断；建议用 `form_urlencoded::Serializer`。
- Spec §3.6「canonicalize 失败提示」在 Task 6 add handler 里是静默 fallback（`unwrap_or_else`），与 spec 文案略不符，可接受（与 CLI 行为一致），留意即可。

## 3. 核对通过明细（供执行时对照，逐项已验证）

| Spec 声明 | 验证结果 |
|---|---|
| `npx::find`、`install(paths,source,skill,package,scope)`、`import_existing`、`upgrade_all`、`Project::register`、`proj.rebind`、`proj.apply_profile`+`Profile::load`+`list_profile_names` | 全部存在，签名与 spec 调用一致（`core/src/{npx,install,import,upgrade,project,profile}.rs`） |
| install handler 明确拒绝 registry 源 | 属实（`crates/server/src/routes/skills.rs:85-102`，`package=None → 400`） |
| 错误变体 `SkillAlreadyInstalled`/`UpgradeBlocked`/`ProjectNotFound`/`Tool` | 全部在 `crates/core/src/error.rs` 定义 |
| Task 1 的 lib.rs re-export 行（`project.rs:28` 原样） | 与计划逐字一致（`pub use project::{list_ids as list_project_ids, Project};`） |
| fake npx 参数位置（find $2、add $5=skill、update $3=skill） | 与 `npx.rs` 真实命令完全对位 |
| install 的 spec↔skill 语义（`skills.sh/pdf` id、canonical 名=query） | 与 CLI `resolve_registry_package`（`cli/commands/install.rs:38`）+ `Registry::skill_id` 一致 |
| import 登记 `unmanaged/foo`（Task 4 断言） | 与 `core/src/import.rs:91-103` 一致 |
| upgrade-all 的 `UpgradeAllReport{upgraded,blocked}`（Task 5 summary） | 与 `core/src/upgrade.rs:51-55` 一致 |
| Task 10 依赖的 `fragment_response_is_main_content_only` 契约测试 | 已存在（`crates/server/tests/routes.rs:121`），且只查 `?fragment=1` 无 nav——新加 find-bar/按钮不会破坏 |
| Task 6 的 `Config::load` 缺省行为（agents 默认） | 缺 config.toml 时返回默认（仅 claude-code），`Config::load` 不报错 |

## 4. 修正建议的执行顺序

1. **改 plan**（`docs/superpowers/plans/2026-08-01-gui-parity.md`）再动手：
   - Task 9 Step 5：`hx-target="body"` → `hx-target="#status-panel" hx-swap="outerHTML"`；补一条断言响应含 `status-panel`。
   - Task 5 Step 3：`upgrade_all(paths, true)` → 决策 `false`（推荐）；Task 5 Step 1 测试加一个锁 oldhash 的 project，断言 blocked 列出受影响项目。
   - Task 2 Step 6 / Task 7 Step 5：`hx-swap="innerHTML"` → `hx-swap="outerHTML"`（保留片段外层固定 id）。
2. 然后按交接 §7.2 逐 task 执行（Task 1→10，依赖顺序在 plan Self-Review 已理清）。

## 5. 结论

Spec 值得按现状进入执行，但建议**先修 §2-P0（Task 9 模板 target，1 行）**再跑 plan；**§2-P1（upgrade_all 参数）在执行 Task 5 时决策**（推荐 `false` + 补锁定项目用例），否则「列出不拦截」在 GUI 是空转。其余为 minor。修正后即可执行。
