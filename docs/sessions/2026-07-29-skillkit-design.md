# 2026-07-29 → 2026-07-31 skillkit（设计 → M0 → M1 → M2 GUI 进行中：T1-T9 完成）

> 用途：skillkit 会话关键事实/决策/遗留沉淀。新会话读 §1 + §4 + §7 三段够用；细节回查 §2/§3/§5/§6。
>
> **M2 进行中**：server crate T1-T9 完成（骨架/token/静态/layout/文件锁/Sources·Skills·Profiles 视图 + Profiles 写操作），剩 T10-T15。

## 1. 当前状态（2026-07-31，M2 GUI 进行中：T1-T9 完成，剩 T10-T15）

### 1.1 命令表面

```
skillkit source/install/uninstall/profile/project     # ✅ M0+M1
skillkit serve [--port 7317]                           # ✅ M2 T1-T9：起 server + Sources/Skills/Profiles 视图可访问
```

M2 剩（plan Task 10-15）：Projects 视图、Sources/Skills 写操作、Projects 声明编辑/apply 闭环、SSE、视觉打磨。详见 `docs/superpowers/plans/2026-07-31-skillkit-m2.md`。

### 1.2 结构性事实

- **server crate（M2 新增）**：Axum 0.8 薄壳调 core。`lib.rs`（serve/run/app/AppState/静态/token 中间件）+ `routes/{mod,sources,skills,profiles}.rs` + `templates/`（layout/home/sources/skills/profiles + fragments/profile_skills）+ `static/`（htmx.min.js/sortable.min.js/app.css，rust-embed 嵌）+ `tests/{common/mod.rs, routes.rs}`（8 个 oneshot 测试）。
- **文件锁（M2 T6）**：`core/lock.rs` 的 `FileLock`（fs2 flock），5 个 save 接入（key：`registry`/`sources`/`config`/`project-{id}`/`profile-{name}`）。读不锁，写 5s 超时报 `LockTimeout`。
- **配置目录改名**：`~/.skm/` → `~/.skillkit/`（M2 起步，全仓库含代码方法名 `skillkit_dir`/`skillkit_skills_dir`）。
- **M0+M1 既有**：core 13 模块（+lock）+ CLI source/install/profile/project/serve + e2e。
- **43 tests 全绿**（core 35 + server 8），clippy `pedantic -D warnings` 零 warning。
- 存储不变量：`~/.agents/skills/` 通用加载目录（不改）；元数据统一 `~/.skillkit/`；单版本；shared 只读；id 引用 DRY；agent 能力 config 驱动。

### 1.3 验证 flow

```bash
cd /Users/mywo/lab/skillkit
make check                              # 43 tests 全绿
make run ARGS="serve --port 7317"       # 起 GUI（打印 http://127.0.0.1:7317/<token>/）
# T1-T9：Sources/Skills/Profiles 可访问；Projects 路由待 T10
```

## 2. 本会话累积的改动（newest first）

8. **M2 GUI 实现 T1-T9**（本轮，12 commit `cca11f4`→`a1ff11a` + spec `2c60460` + plan `e63d7f3`）
   - 方式：主人选 inline 执行（executing-plans），按 plan TDD 红绿 + 每 task commit。
   - T1-T9：server crate 骨架 → token 中间件（`/{token}/`）→ rust-embed 静态 → Askama layout/home → core 文件锁 → Sources/Skills 视图只读 → Profiles 视图 + add/remove + SortableJS 拖拽。
   - 执行中发现 plan 几处写法卡（见 §3.1），已现场修正代码；plan 文档只改了 askama include 等几处，其余坑在 §3.1 记录，下会话按此调整 plan Task 10-15 代码。
   - 验证：43 tests + clippy 零 warning。serve 手动走查留 T15 收尾统一做。

7. **`.skm` → `.skillkit` 改名 + M2 spec/plan**（`cca11f4` + `2c60460` + `e63d7f3`）
   - 配置目录自描述化（主人定）；spec §12 React+Vite → htmx+Askama+SortableJS（主人定，砍 web/ 工程、SSE 简化、文件锁进 M2）。

6-0. M1/M0/工程化/设计（同前，见 `git log`）。

## 3. 关键背景知识

### 3.1 M2 执行经验（最重要——下会话执行 T10-T15 必读，避免重蹈 plan 坑）

plan（`docs/superpowers/plans/2026-07-31-skillkit-m2.md`）Task 10-15 的代码有几处写法执行时会卡，按以下修正：

1. **handler 渲染模式**：plan 写 `match Tpl{...}.render() {...}`——结构体字面量在 match 头致花括号歧义，rustfmt 报错。改：`let rendered = Tpl{...}.render(); match rendered {...}`。
2. **写操作调视图**：plan 的 create/add 等调 `page(state, ...)`——page 第一个参数是 `State<AppState>` extractor，不能传裸 state。改：拆同步 `fn render_xxx(state: AppState, token: String) -> Response`，page 和写操作都调它（私有同步 fn，见 5）。
3. **重复 key 表单**：plan 的 reorder 用 `Form<ReorderForm{order: Vec<String>}>`——serde_urlencoded（axum Form 底层）不支持重复 key→Vec（`order=a&order=b`）。改：`body: Bytes` + `form_urlencoded::parse(&body).filter(|(k,_)| k=="order").map(|(_,v)| v.into_owned()).collect()`（`form_urlencoded` crate，已在 server Cargo.toml）。
4. **askama include**：plan 写 `{% include "x" with var %}`——askama 不支持 with（共享外层模板上下文）。改：外层 for 变量名对齐被 include 模板字段名（如 `{% for profile in profiles %}` + include 无 with）。已在 plan 修。
5. **私有 async fn**：clippy `unused_async` 对私有 async fn 无 await 报错。`render_xxx` 改同步 fn（pub handler 保持 async）。

### 3.2 M2 clippy pedantic 坑（T10-T15 同样适用）
- `case_sensitive_file_extension_comparisons`：`ends_with(".js")` → `Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("js"))`。
- `used_underscore_binding`：字段 `_file` 若在 Drop 用则改名 `file`（下划线前缀 + 使用 = 矛盾）。
- `single_match_else`：两臂 match 改 `if`/`if let`。
- `suspicious_open_options`：`OpenOptions.create(true)` 要声明 `.truncate(true)`。
- `unstable_name_collisions`：fs2 `unlock` 用完全限定 `fs2::FileExt::unlock(&file)`。

### 3.3 Axum 0.8 要点
- 路由参数 `{token}`（非 0.7 的 `:token`）。
- handler 参数顺序：`State`/`Path` 在前，body extractor（`Form`/`Bytes`）最后。
- `State` 要 `Clone`（`Paths` 已加 `#[derive(Clone)]`）。
- 测试用 `tower::ServiceExt::oneshot` 打 router（`tests/routes.rs`），不起真实 TCP。

### 3.4 既有 M0/M1 背景（不变量）
- apply 闭环：`compute_diff` → `land_one` → `run_apply` → `build_status`；agent_dir 映射 `claude-code`→`.claude`。
- project-id = uuid v4 前 8 hex；`locked_shas` 是上次 apply 基线快照（非版本锁）。
- skills_dir：一仓库多 skill（Source 加 `skills_dir`）。

## 4. 验证清单（重载 / 切换后立即跑）

- [ ] `git -C /Users/mywo/lab/skillkit log --oneline -15` 看到 M2 T1-T9（最新 `a1ff11a` Profiles）。
- [ ] `cd /Users/mywo/lab/skillkit && make check` 全绿（43 tests：core 35 + server 8）。
- [ ] `make run ARGS="serve --port 7317"` 起 server，浏览器打开打印的 URL（Sources/Skills/Profiles 可访问）。
- [ ] `ls crates/server/src/routes/` 看到 `sources/skills/profiles + mod`；`ls crates/server/templates/` 看到 layout/home + 四视图模板 + fragments/。
- [ ] **回归信号**：`cargo clippy --all-targets -- -D warnings` 零 warning；无 `~/.skillkit/.lock/*.lock` 残留（写操作 drop 释放）。

## 5. 已知遗留 / 待办

1. **M2 剩 T10-T15**（下次接续，plan Task 10-15）：
   - T10 Projects 视图只读（列表 + 工作台三栏：installed/shared/status）+ core `scan_shared`（apply.rs，扫项目 agents skills 目录下真实目录）。
   - T11 Sources/Skills 写操作（CRUD + install/uninstall）。
   - T12 Projects 声明编辑（勾选全量替换 `installed_skills` + status 片段端点）。
   - T13 Projects apply 闭环（`run_apply` + ApplyReport 片段）。
   - T14 SSE（notify watcher → changed 事件 → 前端 hx-sse 刷新）。
   - T15 视觉打磨（app.css 产品化 demo 风格）+ M2 收尾（更新 spec §12/CLAUDE.md §2 + 本 sessions）。
2. **M3 迁移打磨**：`import-existing` / `install upgrade` / Brewfile 打包。
3. **基建债**：CI（GitHub Actions `make check`）、README、Cargo.toml `[package]` 元数据。

## 6. 关键文件路径速查

```
/Users/mywo/lab/skillkit/
├── CLAUDE.md / Cargo.toml / Makefile / rustfmt.toml
├── crates/
│   ├── core/                  # skillkit-core（lib）—— 业务逻辑（M0+M1+M2 文件锁）
│   │   └── src/{lib,paths,error,config,source,registry,git,install,symlink,profile,project,apply,lock}.rs
│   ├── cli/                   # skillkit-cli（bin）—— 薄壳
│   │   └── src/{main, commands/{source,install,profile,project,serve}}.rs
│   └── server/                # skillkit-server（lib，M2 新增）—— Axum 薄壳
│       ├── Cargo.toml         # axum0.8 / askama0.13 / rust-embed8 / fs2 / form_urlencoded / uuid / tokio（notify 待 T14 加）
│       ├── src/{lib.rs, routes/{mod,sources,skills,profiles}.rs}
│       ├── templates/{layout,home,sources,skills,profiles}.html + fragments/profile_skills.html
│       ├── static/{htmx.min.js, sortable.min.js, app.css}
│       └── tests/{common/mod.rs, routes.rs}
├── docs/
│   ├── 2026-07-29-skillkit-design.md          # spec（§12 待 T15 改 htmx 定稿）
│   ├── design-decisions-2026-07-29.md
│   ├── superpowers/specs/2026-07-31-skillkit-m2-design.md   # M2 spec（htmx 路线）
│   ├── superpowers/plans/2026-07-31-skillkit-m2.md          # M2 plan（15 task，T1-T9 已执行）
│   └── sessions/2026-07-29-skillkit-design.md               # 本交接
```

## 7. 下次接续工作的最短路径（M2 T10-T15）

### 7.1 冷启动（新会话第一件事）

```bash
cd /Users/mywo/lab/skillkit
git log --oneline -15                   # 确认 M2 T1-T9（最新 a1ff11a）
make check                              # 43 tests 全绿
sed -n '/### Task 10/,/### Task 16/p' docs/superpowers/plans/2026-07-31-skillkit-m2.md  # 读 T10-T15
```

**必读本文件 §3.1（M2 执行经验）**：plan Task 10-15 代码有 4 处坑（match-字面量 / 写操作调 handler / serde_urlencoded 不支持重复 key→Vec / 私有 async fn），按 §3.1 修正模式实现，否则会卡在 rustfmt/clippy/422。

### 7.2 焦点：T10 Projects 视图 → T15 视觉

按 plan Task 10-15 TDD 红绿 + 每 task commit（沿用 T1-T9 节奏：cargo test 验证逻辑 + 攒到节点 make check 查 clippy/format）。关键：

- **T10** core 加 `pub fn scan_shared(project_root, agents) -> Vec<String>`（apply.rs，找 agents skills 目录下真实目录、非 symlink、无 `.skillkit-sha`）+ `lib.rs` re-export；`routes/projects.rs`（list + workspace 三栏）。
- **T11** Sources add/remove + Skills install/uninstall（写操作后返回视图整页，用 §3.1-2 的拆 `render_xxx` 模式）。
- **T12** Projects 声明编辑：`set_skills`（全量替换 `installed_skills`）+ `status` 片段端点（供 SSE hx-get）。
- **T13** apply：`skillkit_core::apply::run_apply(&paths, &mut proj, false)` → `ApplyReport` + 刷新 status。
- **T14** SSE：notify v8 `RecommendedWatcher`（同步，包 `std::thread`）+ `tokio::sync::mpsc` + `axum::response::sse`。前端 hx-sse 需 htmx sse 扩展（额外下 `static/ext/sse.js`）。SSE 自动化测试难，手动验证（spec §10 已注明）。
- **T15** 收尾：`make check` 全绿 + 手动 `serve` 走通四视图 + 更新 spec §12/CLAUDE.md §2（React→htmx 定稿）+ 本 sessions。

### 7.3 焦点优先级

1. T10-T13（Projects 视图 → apply 闭环，核心）→ 2. T14 SSE → 3. T15 视觉 + 收尾 → 4. M3 迁移 → 5. 基建债。

## 7.x (archive) 历史接续路径

- **M2 T1-T9 阶段（已完成）**：inline 执行 plan Task 1-9，server 骨架 → Profiles 写操作（含 SortableJS）。
- **M2 设计阶段（已完成）**：brainstorming htmx（取代 React）→ spec → writing-plans 15 task。
- **M1 阶段（已完成）**：profile/project/apply 幂等落地闭环（10 TDD task）。
- **M0 阶段（已完成）**：core 骨架 + install + 全局 Claude symlink。
- **设计阶段（已完成）**：CLAUDE.md → spec review（P0/P1/P2）→ writing-plans M0。
