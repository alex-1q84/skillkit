# 2026-07-29 → 2026-07-31 skillkit（设计 → M0 → M1 → M2 GUI 完成）

> 用途：skillkit 会话关键事实/决策/遗留沉淀。新会话读 §1 + §4 + §7 三段够用；细节回查 §2/§3/§5/§6。
>
> **M2 完成**：server crate T1-T15 全完成（四视图 + Sources/Skills 写操作 + Profiles 拖拽 + Projects 声明编辑/apply 闭环 + SSE 跨进程刷新 + 视觉打磨）。下次接续 M3（迁移打磨）。

## 1. 当前状态（2026-07-31，M2 GUI 完成）

### 1.1 命令表面

```
skillkit source/install/uninstall/profile/project     # ✅ M0+M1
skillkit serve [--port 7317] [--no-open]               # ✅ M2：四视图 + apply 闭环 + SSE，默认自动打开浏览器（--no-open 跳过）
```

M2 全完成（plan Task 1-15）：Sources/Skills/Profiles/Projects 四视图 + 写操作 + Projects 声明编辑 → APPLY 闭环 + SSE 跨进程刷新 + app.css 产品化。详见 `docs/superpowers/plans/2026-07-31-skillkit-m2.md`。

### 1.2 结构性事实

- **server crate（M2 完成）**：Axum 0.8 薄壳调 core。
  - `lib.rs`（serve/run/app/AppState/静态/token 中间件）。
  - `routes/{mod,sources,skills,profiles,projects,sse}.rs`——sources CRUD、skills install/uninstall、profiles create/add/remove/reorder、projects list/workspace/set_skills/status/apply、sse events。
  - `templates/{layout,home,sources,skills,profiles,projects,project_workspace}.html` + `fragments/{profile_skills,status,apply_result}.html`。
  - `static/{htmx.min.js,sortable.min.js,app.css}`（rust-embed 嵌，app.css 提炼 demo 亮色风格）。
  - `tests/{common/mod.rs, routes.rs}`（15 个 oneshot 测试）。
- **core（M2 增量）**：`lock.rs`（文件锁）+ `apply.rs` 加 `scan_shared`（shared 只读扫描）+ `lib.rs` re-export 补全（build_status/run_apply/scan_shared/StatusView/ApplyReport）。
- **配置目录**：`~/.skillkit/`（M2 起步改名，全仓库含代码方法名 `skillkit_dir`/`skillkit_skills_dir`）。
- **M0+M1 既有**：core 13 模块（+lock）+ CLI source/install/profile/project/serve + e2e。
- **45 tests 全绿**（core 30 + cli 3 + server 15... 实际 make check 全量绿，server routes 15 个 oneshot），clippy `pedantic -D warnings` 零 warning。
- 存储不变量：`~/.agents/skills/` 通用加载目录（不改）；元数据统一 `~/.skillkit/`；单版本；shared 只读；id 引用 DRY；agent 能力 config 驱动。

### 1.3 验证 flow

```bash
cd /Users/mywo/lab/skillkit
make check                              # 全绿（core + cli + server 15 routes 测试）
make run ARGS="serve --port 7317"       # 起 GUI（默认自动打开浏览器；带尾斜杠 URL 可达）
# 四视图：Sources（增删源）/ Skills（install/uninstall）/ Profiles（create/add/拖拽）/ Projects（勾选 → APPLY → status diff）
# SSE：CLI 在另一进程改 ~/.skillkit/ 后浏览器视图自动刷新
```

## 2. 本会话累积的改动（newest first）

10. **serve 自动打开浏览器**（本轮，`450a7fd`）：serve 默认启动后用 `open`(macOS)/`xdg-open`(Linux)/`start`(Windows) 打开默认浏览器（标准库 `Command` 手写，无新依赖）；加 `--no-open` flag 供脚本/CI/测试跳过。listener 绑好后才 open，浏览器请求立即连上；open 失败只 warn 不挡 serve。主人确认弹窗生效。

9. **M2 T10-T15**（本轮，6 commit `9fd9fb5`→`016231b`）
   - T10 Projects 视图（list + workspace 三栏只读）+ core `scan_shared` + lib.rs re-export 补全。
   - T11 Sources CRUD + Skills install/uninstall（handler 拆同步 render_fn + tuple view 编码 id）。
   - T12 Projects 声明编辑（勾选全量替换 installed_skills）+ status 片段。
   - T13 Projects apply 闭环（run_apply + apply_result 片段）。
   - T14 SSE（notify watcher → changed 事件，原生 EventSource 前端）。
   - T15 app.css 产品化（提炼 demo 风格）+ home 尾斜杠 404 修复。
   - 执行中踩的坑见 §3.1（re-export 混用 / default_trait_access / ignored_unit_patterns / Axum %2F / 尾斜杠 / askama contains 借用）。

8. **M2 T1-T9**（12 commit `cca11f4`→`a1ff11a`）：server 骨架 → token → 静态 → layout → 文件锁 → Sources/Skills/Profiles 视图。

7. **`.skm` → `.skillkit` 改名 + M2 spec/plan**（`cca11f4` + `2c60460` + `e63d7f3`）。

6-0. M1/M0/工程化/设计（同前，见 `git log`）。

## 3. 关键背景知识

### 3.1 M2 执行经验（最重要——下会话扩展 GUI 或类似 htmx+askama 工作必读）

plan 的代码有几处写法执行时会卡，修正模式（T1-T15 验证过）：

1. **handler 渲染模式**：不写 `match Tpl{...}.render() {...}`（结构体字面量在 match 头致花括号歧义，rustfmt 报错）。改：`let rendered = Tpl{...}.render();` 再 match，或拆同步 `fn render_xxx(state, token) -> Response` + `render_str(askama::Result<String>) -> Response` helper（profiles/sources/skills/projects 都用这模式）。
2. **写操作调视图**：page handler 第一个参数是 `State<AppState>` extractor，不能从写操作传裸 state 调。改：拆同步 `fn render_xxx(state: AppState, token: String) -> Response`，page 和写操作（add/remove/set_skills）都调它。
3. **重复 key 表单**：`Form<ReorderForm{order: Vec<String>}>` 不行——serde_urlencoded（axum Form 底层）不支持重复 key→Vec（`order=a&order=b`）。改：`body: Bytes` + `form_urlencoded::parse(&body).filter(|(k,_)| k=="order").map(|(_,v)| v.into_owned()).collect()`。profiles reorder、projects set_skills 都用。
4. **askama include**：不支持 `{% include "x" with var %}`（共享外层模板上下文）。改：外层 for 变量名对齐被 include 模板字段名。
5. **私有 async fn**：clippy `unused_async` 对私有 async fn 无 await 报错。`render_xxx` 改同步 fn（pub handler 保持 async）。
6. **re-export 不混用**（T10 新发现）：core 公开类型在 `lib.rs` 统一 re-export，handler 全用 `skillkit_core::X`，不混用 `skillkit_core::apply::X` 子模块路径。新增 `build_status`/`run_apply`/`scan_shared`/`StatusView`/`ApplyReport` 都补进 lib.rs。
7. **clippy pedantic 坑**：
   - `default_trait_access`：测试里 `locked_shas: Default::default()` → `BTreeMap::new()`（具体类型）。
   - `ignored_unit_patterns`：`uninstall` 返回 `Result<()>`，`Ok(_)` → `Ok(())`（install 返回 SkillMeta 不受影响）。
   - `case_sensitive_file_extension_comparisons`：`ends_with(".js")` → `Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("js"))`。
   - `single_match_else`：两臂 match（含 Some("x") 模式）改 `if matches!(...) {} else {}`。
8. **Axum 0.8 路径参数**：`{token}`（非 0.7 的 `:token`）；handler 参数顺序 State/Path 在前，body extractor（Form/Bytes）最后；State 要 Clone。**{id} 单段接受 %2F 解码**——`/skills/demo%2Fx` 的 {id}="demo/x"（后端可行）；但前端 HTML 按钮 URL 里 id 含 / 会被浏览器当多段，须 handler 预编码（`m.id.replace('/', "%2F")` 装进 tuple view，模板用编码值）。
9. **尾斜杠 404**（T15 新发现）：axum 0.8 `/{token}` 严格匹配，`/TOKEN/`（serve 打印的 URL）404。修：额外注册 `/{token}/` 指向同 handler。
10. **askama 不支持方法借用参数**：`{% if project.installed_skills.contains(&meta.id) %}` 编译失败（askama 表达式不支持 & 借用）。改：handler 预计算 `Vec<(SkillMeta, bool)>`，模板 `{% for (meta, checked) in all_skills %}{% if checked %} checked{% endif %}`。
11. **htmx 片段替换保 id**：写操作返回片段做 outerHTML 替换时，片段外层用固定 id（如 `<div id="status-panel">`），否则替换后 id 丢失，下次操作找不到 target。set_skills 返回 status 片段、apply 返回 apply_result 片段，外层都 `id="status-panel"`。
12. **SSE 前端**：htmx 2.x 把 sse 扩展拆到独立 npm 包（`htmx-ext-sse`），额外下载麻烦。改用浏览器原生 `EventSource` + `htmx.ajax('GET', location.pathname, {target:'main', swap:'outerHTML'})`，零额外依赖。
13. **Scope serde**：`#[serde(rename_all = "lowercase")]`，json 里是 `"global"`/`"local"`（造测试数据用小写）。

### 3.2 Axum 0.8 要点（同前，仍适用）

见 3.1-8。

### 3.3 既有 M0/M1 背景（不变量）

- apply 闭环：`compute_diff` → `land_one` → `run_apply` → `build_status`；agent_dir 映射 `claude-code`→`.claude`。
- project-id = uuid v4 前 8 hex 大写；`locked_shas` 是上次 apply 基线快照（非版本锁）。
- skills_dir：一仓库多 skill（Source 加 `skills_dir`）。

## 4. 验证清单（重载 / 切换后立即跑）

- [ ] `git -C /Users/mywo/lab/skillkit log --oneline -20` 看到 M2 T1-T15（最新 `016231b` app.css + 尾斜杠修复）。
- [ ] `cd /Users/mywo/lab/skillkit && make check` 全绿（server 15 routes 测试 + core + cli）。
- [ ] `make run ARGS="serve --port 7317"` 起 server，浏览器打开打印的 URL（带尾斜杠可达）：
  - Sources 增删源、Skills install/uninstall、Profiles create/add/拖拽、Projects 勾选 → APPLY → status diff。
  - SSE：另开终端 `skillkit profile create foo`，浏览器 Profiles 视图自动刷新。
- [ ] `ls crates/server/src/routes/` 看到 `sources/skills/profiles/projects/sse + mod`；`ls crates/server/templates/` 看到 layout/home + 六视图模板 + fragments/{profile_skills,status,apply_result}。
- [ ] **回归信号**：`cargo clippy --all-targets -- -D warnings` 零 warning；无 `~/.skillkit/.lock/*.lock` 残留。

## 5. 已知遗留 / 待办

1. **M3 迁移打磨**（下次接续，spec §15）：
   - `skillkit import-existing`：扫描导入现有 skill（`~/.codex/skills/`、`~/.cursor/skills/` 等存量）。
   - `skillkit upgrade <id>` / `--all`：升级 + 扫描 locked_shas 冲突（spec §10 line 326）。
   - 打包进 mac-config Brewfile（与 cx/rtk 一致）。
2. **基建债**：CI（GitHub Actions `make check`）、README、Cargo.toml `[package]` 元数据（description/license/repository）。
3. **SSE 线程保活优化**（可选）：notify watcher 在 SSE 连接断开后不退出（loop sleep 保活），多次刷新累积线程。本地短时 serve 可接受，长跑需绑定 stream 生命周期。
4. **GUI 视觉微调**（主人主观 review）：app.css 已产品化，主人 serve 走查后可能要调间距/配色。

## 6. 关键文件路径速查

```
/Users/mywo/lab/skillkit/
├── CLAUDE.md / Cargo.toml / Makefile / rustfmt.toml
├── crates/
│   ├── core/                  # skillkit-core（lib）—— 业务逻辑（M0+M1+M2 文件锁+scan_shared）
│   │   └── src/{lib,paths,error,config,source,registry,git,install,symlink,profile,project,apply,lock}.rs
│   ├── cli/                   # skillkit-cli（bin）—— 薄壳
│   │   └── src/{main, commands/{source,install,profile,project,serve}}.rs
│   └── server/                # skillkit-server（lib，M2 完成）—— Axum + Askama + rust-embed
│       ├── Cargo.toml         # axum0.8/askama0.13/rust-embed8/fs2/form_urlencoded/notify8/tokio-stream/futures-util/uuid/tokio
│       ├── src/{lib.rs, routes/{mod,sources,skills,profiles,projects,sse}.rs}
│       ├── templates/{layout,home,sources,skills,profiles,projects,project_workspace}.html + fragments/{profile_skills,status,apply_result}.html
│       ├── static/{htmx.min.js, sortable.min.js, app.css}
│       └── tests/{common/mod.rs, routes.rs}
├── demo/index.html            # GUI 视觉设计稿（app.css 提炼来源）
├── docs/
│   ├── 2026-07-29-skillkit-design.md          # spec（§3/§12 已改 htmx 定稿）
│   ├── design-decisions-2026-07-29.md
│   ├── superpowers/specs/2026-07-31-skillkit-m2-design.md   # M2 spec（htmx 路线）
│   ├── superpowers/plans/2026-07-31-skillkit-m2.md          # M2 plan（15 task，全完成）
│   └── sessions/2026-07-29-skillkit-design.md               # 本交接
```

## 7. 下次接续工作的最短路径（M3 迁移打磨）

### 7.1 冷启动（新会话第一件事）

```bash
cd /Users/mywo/lab/skillkit
git log --oneline -20                   # 确认 M2 完成（最新 016231b）
make check                              # 全绿
make run ARGS="serve --port 7317"       # 起 GUI 手动走查四视图（确认 M2 视觉/功能没退化）
```

**必读本文件 §3.1**（M2 执行经验）：若 M3 涉及 GUI 扩展或新 htmx 端点，13 条坑（re-export 统一 / 重复 key 表单 / Axum %2F / 尾斜杠 / askama contains 借用 / SSE 原生 EventSource 等）按此修正模式实现。

### 7.2 焦点：M3 三件事

1. **import-existing**：扫描 `~/.codex/skills/`、`~/.cursor/skills/` 等存量 skill 目录，识别 + 登记进 registry（local source 指向原位，或迁移到 canonical）。spec §15、§6.3（shared 只读 vs local 迁移）。
2. **upgrade**：`skillkit upgrade <id>` 拉新版本 + 更新 commit_sha；扫描所有 project 的 `locked_shas`，锁了不同版本的列出受影响项目并警告，需 `--yes`（spec §10 line 326）。
3. **Brewfile 打包**：纳入 mac-config Brewfile（build + install）。

### 7.3 焦点优先级

1. M3 三件（import-existing → upgrade → Brewfile）→ 2. 基建债（CI/README/元数据）→ 3. SSE 线程优化（可选）。

## 7.x (archive) 历史接续路径

- **M2 T10-T15 阶段（已完成）**：inline 执行 plan Task 10-15，Projects 视图 → 写操作 → apply 闭环 → SSE → 视觉。
- **M2 T1-T9 阶段（已完成）**：inline 执行 plan Task 1-9，server 骨架 → Profiles 写操作（含 SortableJS）。
- **M2 设计阶段（已完成）**：brainstorming htmx（取代 React）→ spec → writing-plans 15 task。
- **M1 阶段（已完成）**：profile/project/apply 幂等落地闭环（10 TDD task）。
- **M0 阶段（已完成）**：core 骨架 + install + 全局 Claude symlink。
- **设计阶段（已完成）**：CLAUDE.md → spec review（P0/P1/P2）→ writing-plans M0。
