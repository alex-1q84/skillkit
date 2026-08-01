# Spec Review — Projects 管理界面重构设计（2026-08-01）

> 审查对象：`docs/superpowers/specs/2026-08-01-projects-ui-redesign-design.md`
> 审查基准：对代码逐一核对（`crates/core`、`crates/server`、templates、tests、e2e、`docs/frontend-rules.md`）+ 需求 7 条逐项对照。
> 日期：2026-08-01
> 结论：**设计方向与 7 条需求全部对齐，决策 1/2/3 成立且能落地；但 §5 有 3 处与代码库既有模式冲突或自相矛盾，须修正后再进 writing-plans**。详见 §2。

## 1. 总体结论

需求覆盖对照（7/7 全对齐）：

| 需求 | spec 落点 | 评价 |
|---|---|---|
| 注册/扫描明显区分 | §4.1 两张独立 `.card` + 标题 + 一句说明 | ✓ |
| 列表可移除已注册项目 | §4.2 删除按钮 + 决策 3 | ✓ |
| 详情页 status 置顶、名称下方、横向 | §3.1 顶部 status badge 条 | ✓ |
| 展示绑定 profiles | §3.1「绑定：applied_profiles 展示」 | ✓ |
| 绑定多选、卡片、点选、应用 | §3.2 profile 卡片网格 + 应用 | ✓ |
| update/APPLY 是否多余 | 决策 1/2 去掉勾选表单与 update，APPLY 并入「应用」 | ✓ 回答「多余」——是，且根因（`applied_profiles` 与 `installed_skills` 脱节）找得准 |
| 重绑定放 status 下方、同注册的浏览向导 | §3.1 重绑定路径卡 + 复用 browse | ✓ |

三个决策核对通过：

- **决策 1（profile 驱动 skill 声明）**：`compute_diff`（`crates/core/src/apply.rs:30`）本来就只对 local scope 生成 expected、global 走 `ensure_global_claude`（`apply.rs:260-265`）——「local 区块按 scope 过滤」的派生决策与现状一致；`applied_profiles`/`installed_skills` 双字段脱节问题属实。
- **决策 2（应用一步到位）**：`run_apply(paths, proj, false)` 签名对位，落地后 `locked_shas` 更新已在 core 内，薄壳 handler 可行。
- **决策 3（删除只注销）**：无清理 side-effect，符合 CLAUDE.md §5 红线；`ProjectNotFound` 错误变体已存在（`crates/core/src/error.rs:41`）。

## 2. 必须修正 / 需决策的问题

### 🔴 P0 — §5.2 DELETE 返回 `HX-Redirect` 偏离代码库既有删除模式

- 现状所有删除（sources/skills）都是 `hx-delete` + `hx-target="body" hx-swap="outerHTML"`，handler 返回完整页（`routes/sources.rs:119`、`skills_main.html:40`、`sources_main.html:20`）。`HX-Redirect` 全库零使用。
- 与前端硬规则「写操作返回完整页面 body outerHTML」（frontend-rules §1）直接冲突；且列表页删除返回 `HX-Redirect` 会造成 SSE（`changed` → `?fragment=1`）与 `HX-Redirect` 双通道竞态。
- 修正：`remove` handler 改为 `render_list(token, projects, false)`（完整 Projects 页）；模板用 `hx-delete … hx-target="body" hx-swap="outerHTML"`，与 sources/skills 同款。e2e 既有 `button.x` + body 替换断言模式可复用。

### 🟠 P1 — §3.3 「应用」返回工作台 main 片段，spec 自相矛盾 + 新写操作模式

- §3.2 只写「'应用'按钮 submit 整个表单」，未给 `hx-target/hx-swap`；§3.3 却要求返回 fragment。同页 rebind（写操作）返回完整页、应用返回 fragment，模式混杂。
- 前端硬规则与同页 rebind 都指向完整页。修正：`set_profiles` 返回 `render_workspace(state, token, proj, false)`（`report: Some`），表单 `hx-target="body" hx-swap="outerHTML"`。功能无差别（SSE 最终都重渲 main），但少一个模式。
- 若坚持 fragment，须在 §3.2 明确 `hx-target="main" hx-swap="innerHTML"` 并说明为何偏离完整页惯例。

### 🟠 P1 — §5.3「apply_result.html 保留复用」与 §5.2 删 apply 端点自相矛盾

- `apply_result.html` 唯一渲染方是 `apply` handler（`routes/projects.rs:295`）；§5.2 删掉该端点后它就是死代码。
- 修正：删掉 `apply_result.html`，报告区在 `workspace_main.html` 内联渲染（`report.created.len()` 等字段模板直接可访问）。

### 🟡 P2 — SSE watcher 会立刻覆盖「上次应用结果区」，需明示瞬时性

- `set_profiles`/rebind/delete 都写 `projects/<id>.toml` → notify watcher（`sse.rs:88-104`）→ `changed` → GET `?fragment=1` 重渲 main（`report=None`）。报告区不只是「GET 时不渲染」，POST 落地后也会被 SSE 毫秒级覆盖，实际是瞬时反馈。
- 现状 `apply_result.html` 同样如此（留了「回到工作台」链接兜底）。可接受，但 §3.3 步骤 5 表述（「应用后展示」）会误导实现者，应注明「瞬时反馈，落地的 status 才是持久状态」。

### 🟡 P2 — §5.2 路由合并未说明

- `GET /projects/{id}` 与新增 `DELETE /projects/{id}` 同 path，需 `.route("/{token}/projects/{id}", get(workspace).delete(remove))` 合并注册（`delete` 已在 `routes/mod.rs:2` import）。

## 3. 核对通过明细（供执行时对照，逐项已验证）

| Spec 声明 | 验证结果 |
|---|---|
| `Project::apply_profile` / `add_skill` / `remove_skill` 存在，CLI 在用（`cli/commands/project.rs:83`） | 属实；§5.1「保留不删」成立（YAGNI） |
| `run_apply` 幂等 + `locked_shas` 更新 | `apply.rs:240-325`，重复 apply 零 created（测试 `apply.rs:528`） |
| `compute_diff` 只生成 local expected、global 静默 ensure | `apply.rs:37` + `apply.rs:260-265`，测试 `diff_expected_only_local_global_skipped` |
| status 由 `build_status` 给 expected/missing/extra/conflicts | `apply.rs:337`，字段与 StatusView 对位 |
| browse 端点 `GET /{token}/projects/browse` 参数 `into/panel/path/select` | `routes/projects.rs:384-430`；`browse.html`/`browse_select.html` 复用可行 |
| 重绑定 handler `rebind` 接收裸 `path`（复用向导 input `name=path` 直接提交） | `routes/projects.rs:179-192`，form 字段正好 `path`，零改动 |
| `set_profiles` body 重复 key 需 `form_urlencoded::parse` 手动收集 | 同 `set_skills` 现状（`routes/projects.rs:247-264`），frontend-rules §6 已明确 |
| `ProfileCard { name, skill_count, bound }` 需 handler 预计算 | `Registry::load` + `Profile::load` 均在 core，薄壳可做，不复制业务逻辑 |
| local 过滤需 handler 预计算 `local_skills` | 前端规则 §1「方法借用参数」坑（`contains(&meta.id)` 编译失败）——handler 预计算是对的 |
| 列表页 local skill 数（`ProjectRow` 包装方案） | 推荐 §5.4 的 `ProjectRow { project, local_count }` 单一字段，防同序错位 |
| `ProjectNotFound` 错误变体 | `error.rs:41` 已存在 |

## 4. 小项（不阻塞，实现期注意）

- **§5.4 缺 CSS 变更清单**：`.workspace` 三栏 grid（`app.css:238`）要改为块状布局 + 底部两列；status badge 条、profile 卡片网格、已绑高亮样式都不存在。建议 §5.3 或新增一节把 `static/app.css` 列入。
- **`set_profiles` 缺「所选 profile 不存在」分支**：§3.3 步骤 3 应说明该情况（返回 err 片段或 4xx），避免 500。
- **profile 空态**：一个 profile 都没有时卡片网格为空，给一句「去 Profiles 视图创建」引导（符合「反馈引导行动」）。
- **`GET /projects/{id}/status` 端点目前无模板引用**（`status.html` 只被 workspace_main include，SSE 走 `?fragment=1` 而非该端点）——保留没问题，但 §5.2「保留不动」注释里「SSE 触发 hx-get 刷新用」与实际不符，顺手修正。
- **GUI 替换语义 vs CLI 追加语义**：决策 1 后 GUI 绑 profile 即全量替换，CLI `apply_profile` 仍是追加（`project.rs:97-106`）。§5.1 已说明保留理由（YAGNI），建议 §7 已知限制补一句，避免后续维护者困惑。

## 5. 修正建议的执行顺序

1. **改 spec**（`docs/superpowers/specs/2026-08-01-projects-ui-redesign-design.md`）再进 writing-plans：
   - §5.2：DELETE 改返回完整列表页（`HX-Redirect` → `render_list`），标注路由 `.route(...)` 合并写法。
   - §3.3：`set_profiles` 返回 `render_workspace(...)` 完整页（`report: Some`），补「所选 profile 不存在」分支；步骤 5 注明报告区瞬时性。
   - §5.3：`apply_result.html` 从「保留复用」改为「删除，报告区内联」。
   - §5.4：补 `static/app.css` 变更清单（workspace 布局、badge 条、profile 卡片、已绑高亮）。
2. 然后按交接流程产 writing-plans。

## 6. 结论

Spec 值得进入执行，但建议**先修 §2 的三处（DELETE 返回模式、set_profiles 返回模式、apply_result.html 归属）再产 plan**；§2-P2（SSE 覆盖瞬时性）改文案即可。修正是方向性的模式统一，不改变设计决策本身，改动量小。修正后即可执行。
