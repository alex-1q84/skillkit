# Projects 管理界面重构 — 设计

> 本 spec 是 Projects 详情页 + 列表页交互重构的设计意图与决策权威。实现计划由后续 writing-plans 产出。代码层规范见 `CLAUDE.md`，前端强规则见 `docs/frontend-rules.md`。

## 1. 背景与目标

Projects 视图已具备注册 / 扫描 / 详情工作台 / apply 闭环，但交互上有一组暴露给用户的混乱：

- 详情页第一栏「update」（手动勾选 skill）与「APPLY」（落地）两个按钮语义不清，用户分不清差异。
- `Project.applied_profiles` 字段已存在但前端从不展示；`apply_profile()` 只追加 profile 不支持取消，与可被「update」独立修改的 `installed_skills` 脱节——两个字段都改 `installed_skills` 却不同步。
- 详情页 status 是底部第三栏的纵向 `pre` 文本块，不直观；重绑定藏在折叠 `<details>` 里、用裸输入框，与列表页注册表单的路径浏览向导体验不一致。
- 列表页注册与扫描两个表单紧挨，无视觉分区；已注册项目无法从列表移除。

目标：把详情页重构为「绑定 profile → 应用」的单一心智模型，列表页补齐分区与注销能力，整体符合 CLAUDE.md「能一步不拆三步」「反馈引导行动」「渐进式展示」。

## 2. 核心设计决策

三个决策已与主人确认，是本重构的不变量：

**决策 1 — profile 绑定驱动 skill 声明。** `installed_skills` 不再可被手动勾选独立修改，改为「所绑 profiles 的 skills 并集」由绑定关系自动推导。去掉详情页手动勾选表单与 `update` 按钮。理由：消除 `applied_profiles` 与 `installed_skills` 脱节的根因；符合项目定位「按 profile 组织候选集、按项目精确安装」（CLAUDE.md §1）。代价：零散 skill 须先归入某 profile——这正是 profile 的职责。

**决策 2 — 「应用」一步到位。** 详情页点「应用」一次完成：保存 profile 绑定 → 重算 `installed_skills` → `run_apply` 落地到项目目录。合并掉原单独的 APPLY 按钮。理由：声明与落地是技术分层，不应让用户感知；符合「能一步完成的不拆三步」。status 顶部立即反映落地结果作为反馈。

**决策 3 — 删除项目只注销。** 从列表删除已注册项目仅删 `~/.skillkit/projects/<id>.toml`，不碰项目目录任何文件。理由：注销 = 解除 skillkit 管理关系，项目目录是用户领地；已落地的 symlink 仍有效指向 canonical 无害；绝不误伤 shared / git 资产（CLAUDE.md §5 红线）。

**派生决策 — local 区块按 scope 过滤。** 详情页 skill 列表标题用「local installed skills」，且只列 local scope 的 skill（apply 时 per-project 落地的那部分）。global scope skill 绑定后静默走全局 ensure（`run_apply` 内 `ensure_global_claude`），不进项目目录、不在详情页展示——global 不属于单项目。理由：名副其实；global skill 管理在 Skills 视图。

**派生决策 — profile 绑定为替换语义。** 「应用」提交的所选 profile 集合 = 当前绑定（全量替换 `applied_profiles`），而非追加。这是能取消绑定的前提，与决策 1 自洽。

## 3. 项目详情页设计

### 3.1 布局（自上而下四块，去掉原三栏 grid）

```
┌──────────────────────────────────────────────────┐
│ 标题行：project.name                      [删除]  │  右侧删除按钮（详情页删除 → 跳列表）
│ 副信息：path · agents                            │
│ status 横向 badge 条（id=status-panel 固定）      │  expected 数 + 非0 异常项；全同步显「✓ 已同步」
│ 绑定：applied_profiles 展示                      │
├──────────────────────────────────────────────────┤
│ 重绑定路径卡（status 下方）                       │  复用 browse 向导（与列表页注册同款）
│ [新路径] [浏览…] [重绑定] + 就近浏览面板          │
├──────────────────────────────────────────────────┤
│ 绑定 Profile 卡                                   │
│ profile 卡片网格（name + skill 数 + 已绑高亮）    │  点卡片 = 切换选中（多选）
│ [▶ 应用]                                          │  绑定 + 落地一步到位
│ 上次应用结果区（created/removed/warnings）        │  应用后展示，GET 时不渲染
├──────────────────────────────────────────────────┤
│ local installed skills（只读）  │ shared（只读·git 管）│  两列只读展示
└──────────────────────────────────────────────────┘
```

### 3.2 交互细节

- **status badge 条**：横向排列。展示 `expected` 总数 + `missing` / `extra` / `conflicts` 中非 0 的项（橙/红 badge 带计数）；三项全 0 时显示绿色「✓ 已同步」。每个异常 badge 用 `<details>` 包裹，展开看具体 id 清单（渐进式展示）。外层固定 `id="status-panel"`，htmx 替换后 id 不丢（前端强规则）。
- **重绑定**：从原底部折叠 `<details>` 移到 status 卡下方。表单复用 `GET /{token}/projects/browse` 端点（input `id=name=path` + 浏览按钮 + 就近面板），与列表页注册表单完全同款体验。
- **profile 卡片**：每个 profile 一张卡，显示 name + skill 数；已绑定的预选并高亮。卡片本质是伪装成卡片的 checkbox（`name="profiles"`），点击 label 切换选中。「应用」按钮 submit 整个表单。
- **profile 空态**：一个 profile 都没有时，卡片网格区显示引导文案「还没有 profile，去 Profiles 视图创建」（符合「反馈引导行动」）。
- **local installed skills 区块**：只读列出 local scope 的 installed skill（按 scope 过滤）。去掉原全量 checkbox 表单与 `update` / `APPLY` 按钮。
- **删除按钮**：调 `DELETE /{token}/projects/{id}`，`hx-confirm` 原生确认（「注销后该项目不再被 skillkit 管理，已落地文件保留」）。

### 3.3 「应用」数据流

1. 用户在 profile 卡片区勾选一组 profile，点「应用」。
2. `POST /{token}/projects/{id}/profiles`，body 为重复 key `profiles=a&profiles=b`（`form_urlencoded` 手动收集，同 profile reorder 模式）。
3. server 薄壳：load project → load 所选 profiles → `Project::set_profiles(names, profiles)`（core 设 `applied_profiles` + 重算 `installed_skills` 为并集）→ save → `run_apply(paths, &mut proj, false)` 落地。
4. 若所选 profile 有不存在者，返回可读 err 片段引导（「profile X 不存在，先去 Profiles 视图创建」），不 500。
5. 返回完整工作台页（`render_workspace(state, token, proj, false)`，`report: Some`），表单 `hx-target="body" hx-swap="outerHTML"`——写操作返回完整页，遵循 frontend-rules §1，与同页 rebind 模式统一。含更新后的 status、绑定展示、local installed 列表、上次应用结果（`ApplyReport`）。
6. 落地结果区在工作台页内联渲染（`report.created/removed/warnings`）。注意：报告区是 POST 响应内的**瞬时反馈**——POST 写 toml 触发 SSE `changed` → 客户端 GET `?fragment=1` 重渲 main（`report=None`），报告区会被毫秒级覆盖。持久的落地状态看顶部 status badge（`build_status` 实时算，SSE 重渲保留）；报告区只作「刚落地 N 个」的一次性确认。

## 4. 项目列表页设计

### 4.1 布局

```
┌──────────────────────────────────────────────────┐
│ Projects                                          │
│ ┌─ 注册项目 ──────────────────────────────────┐  │  两张独立 .card 分区
│ │ 已知项目路径？直接填路径注册。              │  │  各带标题 + 一句说明
│ │ [路径] [浏览…] [注册] + 浏览面板             │  │
│ └─────────────────────────────────────────────┘  │
│ ┌─ 扫描发现 ──────────────────────────────────┐  │
│ │ 不确定有哪些项目？扫目录树自动发现候选。    │  │
│ │ [根目录] [浏览…] [深度] [扫描] + 浏览面板    │  │
│ │ 扫描结果：候选目录，每条带「注册」          │  │
│ └─────────────────────────────────────────────┘  │
│ 已注册项目                                        │
│ 每行：name(链接进详情) · path · local skill 数 · [×删除] │
└──────────────────────────────────────────────────┘
```

### 4.2 交互细节

- 注册 / 扫描改成两张独立 `.card`（复用现有卡片样式），各带小标题 + 一句话说明区分用途：注册 = 已知路径直接填，扫描 = 不确定时扫目录树发现。视觉明显分区。
- 注册不再手填 agents：按项目痕迹精确探测（配置目录 `.claude`/`.codex`/`.cursor`/`.agents` → 指令文件 `CLAUDE.md`/`AGENTS.md`，未命中回退开源标准 `.agents/`），`proj.agents` 绝不默认全量（见主 spec §7、决策 19）。
- 项目列表每行加删除按钮（复用 `button.x` 样式），`hx-confirm` 原生确认。
- 列表项 skill 数改为只计 local scope（与详情页 local 区块一致）；list handler 加载 registry 过滤计数。

## 5. 变更清单

### 5.1 core（`crates/core/src/project.rs`）

- 新增 `Project::set_profiles(&mut self, names: &[String], profiles: &[Profile])`：设 `applied_profiles = names.to_vec()`；重算 `installed_skills` = 所选 profiles 的 `skills` 并集（去重、保序）。业务逻辑留 core。
- 新增 `pub fn remove(paths: &Paths, id: &str) -> Result<()>`：删 `projects/<id>.toml`（不存在返回 `ProjectNotFound`）。
- 现有 `apply_profile` / `add_skill` / `remove_skill` 方法保留（CLI 与测试可能用，不强制删——YAGNI，不扩大改动面）。

### 5.2 server 端点（`crates/server/src/routes/projects.rs` + `mod.rs`）

新增：
- `POST /{token}/projects/{id}/profiles` → `set_profiles` handler（body 重复 key 收集 → core `set_profiles` → `run_apply` → 返回工作台 main 片段，含 `ApplyReport`）。
- `DELETE /{token}/projects/{id}` → `remove` handler（core `remove` → 重载列表 → `render_list(token, projects, false)` 返回完整 Projects 页）。模板用 `hx-delete … hx-target="body" hx-swap="outerHTML"`，与 sources/skills 删除同款（全库统一写操作模式，避免 `HX-Redirect` 与 SSE `changed` 双通道竞态）。路由与 `GET /{token}/projects/{id}` 同 path 合并注册：`.route("/{token}/projects/{id}", get(projects::workspace).delete(projects::remove))`。

删除（详情页不再调用，保持路由干净）：
- `POST /{token}/projects/{id}/skills`（`set_skills` 手动勾选）。
- `POST /{token}/projects/{id}/apply-profile`（单选 apply-profile）。
- `POST /{token}/projects/{id}/apply`（独立 apply；落地已并入 set_profiles，core 的 `run_apply` 保留）。

保留不动：`rebind`（前端表单换 browse 向导，handler 不变）、`browse`、`workspace`、`list`、`add`、`scan`、`status`（注：当前无模板直接引用它，SSE 刷新走 `?fragment=1` 而非该端点；保留不删，供后续按需用）。

### 5.3 模板（`crates/server/templates/`）

- 改 `fragments/workspace_main.html`：按 §3.1 四块重构。去掉全量 checkbox 表单 + update/APPLY；加 status badge 条、重绑定 browse 向导、profile 卡片网格 + 应用按钮、应用结果区、local/shared 两列只读。
- 改 `fragments/status.html`：纵向 `pre` → 横向 badge 条（外层 `id="status-panel"` 保留）。
- 改 `fragments/projects_main.html`：注册/扫描两表单各包进 `.card` + 标题 + 说明；项目列表行加删除按钮、skill 数改 local 计数。
- `fragments/browse.html` / `browse_select.html` 保留复用。
- 删 `fragments/apply_result.html`（原唯一渲染方 `apply` handler 已删，避免死代码）；落地报告区在 `workspace_main.html` 内联渲染（`report.created/removed/warnings` 模板直接访问）。对应的 `ApplyResultTpl` 结构体一并删除。

### 5.4 模板结构体（`projects.rs`）

- `WorkspaceTpl` / `WorkspaceMainTpl` 字段调整：
  - `profiles: Vec<String>` → `Vec<ProfileCard>`（`ProfileCard { name, skill_count, bound: bool }`，handler 预计算）。
  - 去掉 `all_skills: Vec<(SkillMeta, bool)>`（手动勾选用，作废）。
  - 新增 `local_skills: Vec<String>`（按 scope 过滤后的 local installed，handler 预计算，避免模板里调 registry）。
  - 新增 `report: Option<ApplyReport>`（GET 时 None，set_profiles 后 Some）。
  - 保留 `status` / `shared` / `project`。
- `ProjectsTpl` / `ProjectsMainTpl` 字段调整：列表页每行显示 local skill 数，handler 需加载 registry 并按 scope 过滤每个 project 的 `installed_skills` 计数。新增 `local_counts: Vec<(String, usize)>`（与 `projects` 同序，project_id → local skill 数），模板按索引取计数渲染；或把列表项包成 `ProjectRow { project, local_count }` 单一字段（后者更不易错序，推荐）。

### 5.5 静态资源（`crates/server/static/app.css`）

- `.workspace` 三栏 grid（`app.css:204`）改为块状布局：status / 重绑定 / profile 卡片自上而下堆叠，底部 local installed + shared 两列。
- 新增 status badge 条样式（横向圆角 badge：expected 中性色、missing/extra/conflicts 橙/红、全同步绿色「已同步」）。
- 新增 profile 卡片网格样式（网格排列、点击切换选中、已绑定高亮边框 + 底色）。
- 复用既有 `.card` / `button.x` / `button.apply` 样式，不引入新设计语言（保持米色 + 网格 + 琥珀 accent 体系）。

## 6. 测试策略

测试验证业务结果，不验证实现细节（CLAUDE.md §8）。

core 单元测试（`crates/core/src/project.rs`）：
- `set_profiles` 重算并集：选 2 个有重叠 skill 的 profile，断言 `installed_skills` = 去重并集、`applied_profiles` = 所选。
- `set_profiles` 替换语义：先绑 A 再绑 B，断言 `applied_profiles` 只剩 B、`installed_skills` 只含 B 的 skills（取消绑定生效）。
- `remove` 删 toml：存在则删、不存在返回 `ProjectNotFound`。

server 集成测试（`crates/server/tests/routes.rs`）：
- 替换原 `project_set_skills_replaces_installed` / `projects_apply_profile_merges_skills` / apply 端点测试为：
  - `set_profiles` 端点：POST profiles=a&profiles=b → 断言 `installed_skills` 更新 + 落地（项目目录出现 symlink）+ 响应含 status 片段与绑定展示。
  - `remove` 端点：DELETE → 断言 toml 删除 + 响应含 `HX-Redirect`。
- 渲染测试：详情页含 status badge / profile 卡片 / local 过滤；列表页含两张分区卡片 + 删除按钮 + local skill 数。

e2e（`e2e/test_ui.py`）：检查现有用例是否依赖被删按钮文本（update/APPLY）或旧 status 形态，按需更新选择器；可选加一条「选 profile 卡片 → 应用 → 落地」端到端用例。

## 7. 已知限制与非目标

- **global skill 静默处理**：绑定的 profile 含 global scope skill 时，该 skill 进 `installed_skills` 但不在详情页 local 区块展示，apply 时走全局 ensure。global 的可见管理在 Skills 视图，本重构不涉及。
- **set_profiles 全量替换**：`installed_skills` 完全由所选 profiles 决定，无法保留「不属于任何 profile 的额外 skill」——这是决策 1 的必然结果，符合 profile 驱动定位。
- **删除不清理落地**：注销后项目目录的 skillkit-local symlink/copy 保留（决策 3）。用户想清理须手动。
- **非目标**：不改 profile 本身的 CRUD（Profiles 视图）、不改 CLI 命令、不引入 global skill 的项目级展示。
- **路径编码**：browse 向导沿用现状（路径含空格/中文/`&` 时 query 不 percent-encode，YAGNI，项目目录通常无空格）。
- **GUI 替换 vs CLI 追加语义**：决策 1 后 GUI 绑 profile 是全量替换 `installed_skills`；CLI `project apply <profile>`（`crates/core/src/project.rs:97` 的 `apply_profile`）仍是追加语义。两者面向不同场景（GUI 重设绑定、CLI 增量补绑），暂不统一；CLI 语义调整不在本重构范围。
