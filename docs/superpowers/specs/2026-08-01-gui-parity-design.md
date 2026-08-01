# GUI 对齐 CLI 全功能 — 设计文档

> 日期：2026-08-01
> 范围：把 CLI 已有、GUI 缺失的 8 条操作补到 web GUI，使 GUI 与 CLI 能力对齐。
> 代码层规范见 `CLAUDE.md`；本文档是本次完善的设计权威。

## 1. 背景与差距

skillkit 是 CLI（供 AI agent 高频调用）+ 本地 web GUI（供人总览配置）共享同一 core 的工具。当前 GUI 落后于 CLI：CLI 共 18 条原子操作，GUI 已覆盖 10 条，缺 8 条。

差距清单（CLI 有、GUI 无）：

| # | CLI 操作 | core 现状 | GUI 痛点 |
|---|---------|----------|---------|
| 1 | `find <query>` 搜 skills.sh 候选 | `npx::find` ✅ | Skills 视图无搜索框，不能在网页发现 skill |
| 2 | `install add skills.sh <skill>`（registry 源，find 选候选） | `install(...,spec,...)` ✅ | install handler 明确拒绝 registry 源，必须切 CLI |
| 3 | `import-existing` 导入存量 | `import_existing` ✅ | 存量 skill 首次导入必须 CLI |
| 4 | `upgrade --all` 批量升级 | `upgrade_all` ✅ | 只能逐条 upgrade |
| 5 | `project add` 注册项目 | `Project::register` ✅ | Projects 视图只读，无法网页注册 |
| 6 | `project scan` 扫描发现 | ❌ 逻辑在 `cli/commands/project.rs` | 无法网页扫描 |
| 7 | `project rebind` 重绑定 | `proj.rebind` ✅ | 项目移动/改名后无法网页修正 |
| 8 | `project apply-profile` 灌入 profile | `proj.apply_profile` + `Profile::load` ✅ | profile 配好无法一键灌入项目，profile↔project 桥梁断 |

第一性原理：GUI 是「供人总览配置」，凡让人必须切回 CLI 才能完成的操作都是体验漏洞。其中 1+2 断了「发现→安装」主线，5 让 Projects 工作台无法网页自助启动，8 让 profile 失去对项目的落点——这三条最伤。

## 2. 目标与非目标

**目标**

- 8 条操作全部补到 GUI，GUI 与 CLI 能力对齐，无死角。
- 严格遵循三层架构：新端点都是薄壳调 core，零业务逻辑泄漏到 handler/template。
- 遵循前端强规则（§7.5）：htmx 片段、写操作返回 body outerHTML、片段外层固定 id、SSE 刷新走 `?fragment=1`。

**非目标（YAGNI，明确不做）**

- 不给 GUI 端点加 `--json`。`--json` 是 CLI 给 AI agent 的公开契约，GUI 是 web 页面纯片段渲染，二者职责不同。
- find 不做异步任务 + SSE 推送。同步 htmx 请求 + loading 提示足够（见 §6 取舍）。
- 不重构现有已覆盖的 10 条操作（sources/find 之外的 install/list/remove/upgrade 单条/profile 全集/project 的 list/status/apply/勾选增删）。
- 不动 home 占位页（不是 CLI 功能对照）。

## 3. 设计

### 3.1 core 下沉：`scan_projects`（缺口 6 的前置）

`scan_projects(dir, depth)` 当前是 `cli/commands/project.rs` 的私有函数，违反「业务逻辑只在 core」（CLAUDE.md §5）。下沉到 `core/src/project.rs`：

```rust
/// 扫描目录树，返回含 .git 的项目目录（depth 限制递归深度）。
pub fn scan_projects(dir: &Path, depth: u32) -> Result<Vec<PathBuf>>
```

CLI `project scan` 改为调 core，删除 cli 层副本。逻辑不变（递归找 `.git`，跳过 `.git` 自身子目录）。

### 3.2 Skills 视图（缺口 1/2/3/4）

视图结构自上而下：

1. **搜索框**（新增）：输入 query（如 `pdf`），`hx-get="/skills/find?q="`，`hx-target` 指向候选区，`hx-indicator` 显示 loading。
2. **候选结果区**（新增，初始空）：`find_results.html` 片段，每条候选一行，显示 `spec`（`anthropics/skills@pdf`）+ skills.sh 链接 + scope 下拉 + install 按钮。
3. **已装 skill 表格**（现有 SkillsMain，不动）：unmanaged badge / 固定源 install / upgrade / ×。

**find 候选 install 闭环**（缺口 1+2）：
- 候选 install 按钮 → `POST /skills/install-candidate`，body：`spec`（=candidate.spec，如 `anthropics/skills@pdf`）+ `skill`（=搜索时的 query，如 `pdf`）+ `scope`。
- handler 调 `install(paths, "skills.sh", &skill, &spec, scope)`——source 固定 `skills.sh`，package 用 candidate.spec，skill 名用 query。与 CLI `resolve_registry_package` 语义一致。
- 成功 → 刷新 SkillsMain（新装的 skill 进表格）。

**导入存量**（缺口 3）：
- 表格区上方加「导入存量 skill」按钮 → `POST /skills/import`。
- handler 调 `import_existing(paths, false)` → 返回 SkillsMain + 结果计数摘要（imported/unmanaged/reinstalled/skipped 各 N）。

**全部升级**（缺口 4）：
- 表格区上方加「全部升级」按钮 → `POST /skills/upgrade-all`。
- handler 调 `upgrade_all(paths, false)`（对齐 CLI `--all` 默认语义：冲突 skill 只列出不升级，避免锁了 oldhash 的项目基线漂移而零反馈）→ 返回 SkillsMain + 结果摘要（升级 N 个、跳过 M 个冲突并列出受影响项目）。
- 冲突的 `blocked` 列表不静默，列出受影响项目 id（反馈引导行动：提示去对应项目 apply）。

### 3.3 Projects 视图（缺口 5/6/7/8）

**注册项目**（缺口 5）：
- 列表页（`projects.html`）顶部加表单：path 输入框 + agents（可选，默认 config 全 agent）→ `POST /projects`。
- handler：`path.canonicalize()` → `Project::register(abs, agents)` → `save` → 刷新列表。

**扫描发现**（缺口 6）：
- 列表页加「扫描目录」表单：dir 输入框 + depth（默认 3）→ `POST /projects/scan`。
- handler 调 `core::scan_projects(dir, depth)` → 渲染 `scan_results.html` 片段，每条目录一行带「注册」按钮（复用 `POST /projects`，path 预填）。

**重绑定**（缺口 7）：
- 工作台（`project_workspace.html`）加 rebind 表单：新 path 输入 → `POST /projects/{id}/rebind`。
- handler：`proj.rebind(&path)` → `save` → 刷新工作台。

**应用 profile**（缺口 8）：
- 工作台加下拉（列已建 profile，`list_profile_names`）+ 「应用」按钮 → `POST /projects/{id}/apply-profile`。
- handler：`Profile::load(paths, name)` 拿 `.skills` → `proj.apply_profile(name, &skills)` → `save` → 刷新 status 片段（installed_skills 增多，diff 变化）。

### 3.4 新增路由总表

全部遵循现有 `/{token}/` 前缀 + token 校验中间件。

| 方法 | 路径 | handler | core 调用 | 返回 |
|------|------|---------|----------|------|
| GET | `/skills/find` | `skills::find` | `npx::find(paths, q)` | `find_results.html` 片段 |
| POST | `/skills/install-candidate` | `skills::install_candidate` | `install(paths,"skills.sh",skill,spec,scope)` | SkillsMain（body outerHTML） |
| POST | `/skills/import` | `skills::import` | `import_existing(paths,false)` | SkillsMain + 摘要 |
| POST | `/skills/upgrade-all` | `skills::upgrade_all` | `upgrade_all(paths,false)` | SkillsMain + 摘要 |
| POST | `/projects` | `projects::add` | `Project::register`+`save` | 列表（body outerHTML） |
| POST | `/projects/scan` | `projects::scan` | `scan_projects(dir,depth)` | `scan_results.html` 片段 |
| POST | `/projects/{id}/rebind` | `projects::rebind` | `proj.rebind`+`save` | 工作台（body outerHTML） |
| POST | `/projects/{id}/apply-profile` | `projects::apply_profile` | `Profile::load`+`proj.apply_profile`+`save` | `#status-panel` 片段（hx-target=`#status-panel`，非 body） |

GET 类（find、scan 结果）返回局部片段；POST 写类按 §7.5 返回完整页面 body outerHTML。

### 3.5 新增模板清单

- `fragments/find_results.html` — find 候选列表，每条带 install 表单。
- `fragments/scan_results.html` — scan 目录列表，每条带注册按钮。
- 操作结果摘要（import/upgrade-all 的计数）：内联在返回的 SkillsMain 顶部一行。给 `SkillsMainTpl` 加一个可选 `summary: Option<String>` 字段承载（find/install 等不带回摘要时为 None，不渲染该行），不单独建模板（避免过度拆分）。
- `skills.html` / `fragments/skills_main.html` 增搜索框 + 候选区容器（固定 id）+ 导入/全升级按钮。
- `projects.html` / `fragments/projects_main.html` 增注册表单 + 扫描表单。
- `project_workspace.html` / `fragments/workspace_main.html` 增 rebind 表单 + apply-profile 下拉。

### 3.6 错误处理与 loading

- **错误片段化**：core `thiserror` 已有 `SkillAlreadyInstalled`/`UpgradeBlocked`/`ProjectNotFound`/`Tool`(npx 失败) 等。handler 捕获后渲染可读错误片段（不只返回 500），遵循「反馈引导行动」：
  - 已装 →「该 skill 已安装，可在列表中 upgrade 或 remove」
  - find 空/失败 →「在 skills.sh 未找到，换个关键词或检查网络/Node」
  - project 不存在 → 404 片段
- **loading**：find / import / upgrade-all / scan 四个可能慢的操作用 `hx-indicator` + 现有 `.htmx-request` CSS。
- **原子写**：新增的 `projects::add`/`rebind`/`apply_profile` 复用 `Project::save`（已内置文件锁 + atomic_write）。

## 4. find 同步 vs 异步的取舍

find 调 `npx skills find` 是同步阻塞子进程 + 网络，典型 2-5 秒。

- **选同步 htmx + `hx-indicator` loading**。理由：npx find/add 带 `-y` 非交互不会卡死等待输入；find handler 用 `tokio::task::spawn_blocking` 把同步 `npx::find`（`Command::output()`）卸到 blocking 线程池（默认上限 512），不占用 tokio 工作线程（默认 = CPU 核数），不阻塞其他请求；async 任务 + SSE 推送需引入任务表/状态机，是过度设计（YAGNI）。
- 兜底：若实测 find 偶发超 10s，再评估加客户端 `hx-trigger="delay:..."` 或服务端超时，不在本次预设。

## 5. 测试策略

遵循「测试验证业务结果，不验证实现细节」（CLAUDE.md §8）。

- **core 下沉**：`scan_projects` 在 `core/src/project.rs` 加单元测试（tempdir 造含/不含 `.git` 的目录树，断言扫描结果 + depth 截断）。
- **server 集成测试**（`crates/server/tests/`，已有 `routes.rs` 范式）：
  - find/install-candidate：用 fake npx（复用 `core/src/upgrade.rs` 的 `install_fake_npx` 范式），断言候选渲染 + install 后 registry 出现新 skill。
  - import：tempdir 造 `~/.agents/skills/foo`，POST /skills/import，断言 registry 出现 `unmanaged/foo`。
  - upgrade-all：fake npx，断言无冲突 skill 正常升级 + 被项目锁定的 skill 进 blocked 列出（hash 不变、summary 反馈受影响项目）。
  - project add/scan/rebind/apply-profile：tempdir 造项目目录 + profile，断言 project.toml 落地正确、scan 发现 .git 目录、rebind 后 path 更新、apply-profile 后 installed_skills 增多。
- **--json schema 不涉及**：GUI 端点无 JSON 输出，不破坏 CLI 的 `--json` 契约测试。
- 改完跑 `make check`（format + lint + test）双绿；模板改动额外 `make check` 能暴露 Askama 编译错。

## 6. 验收标准

- 8 条 CLI 操作在 GUI 均有对应入口，全程不切 CLI 即可完成。
- 新端点零业务逻辑泄漏（handler/template 只组装参数 + 渲染，推导在 core）。
- `scan_projects` 下沉到 core，cli 层无副本。
- `make check` 双绿，新增集成测试覆盖 8 条操作的正常路径 + 至少一种错误路径（如 install 已装、find 空）。
- 现有 10 条已覆盖操作行为不变（回归不破）。
- 前端强规则（§7.5）全部遵守：写操作 body outerHTML、片段外层固定 id、SSE 刷新不含 nav。

## 7. 风险

- **find 偶发慢/失败**：npx 首次拉包或网络差时可能超 10s。缓解：loading 提示 + 可读错误片段；超时方案留作后续评估。
- **scan 输入路径有效性**：浏览器无法选目录，手输路径可能拼错。缓解：handler 对不存在目录返回可读错误；canonicalize 失败提示。
- **模板膨胀**：Skills/Projects 视图元素增多。缓解：结果摘要内联不单独建模板；遵循「页面薄壳 + include fragment」。
