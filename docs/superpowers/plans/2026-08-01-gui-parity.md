# GUI 对齐 CLI 全功能 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 CLI 已有、GUI 缺失的 8 条操作补到 web GUI（Skills 视图 4 条 + Projects 视图 4 条），并把 `scan_projects` 从 cli 层下沉到 core。

**Architecture:** 严格三层：新端点都是 server 薄壳调 core，零业务逻辑泄漏到 handler/template。core 仅新增 `scan_projects`（从 cli 移入），其余 7 条复用现有 core API。前端 htmx 片段渲染，写操作返回 body outerHTML，慢操作（find/import/upgrade-all/scan）走同步请求 + `hx-indicator` loading。

**Tech Stack:** Rust 2021 + Axum + Askama + htmx + rust-embed；测试用 `tower::ServiceExt::oneshot` + `tempfile` + fake npx（PATH 前置 RAII guard）。

## Global Constraints

- 路径绝不硬编码：用 `dirs::home_dir()` / `Paths`，不写死 `/Users/...`（CLAUDE.md §7）。
- core 公开类型在 `lib.rs` 完整 re-export（CLAUDE.md §7）。
- 文件原子写：复用 `Project::save`（已内置 `FileLock` + `atomic_write`）。
- 前端强规则（§7.5）：写操作（POST）返回完整页面 `hx-target="body" hx-swap="outerHTML"`；GET 片段（find/scan 结果）返回局部片段；片段外层固定 id；SSE 刷新 `?fragment=1` 不含 nav。
- 错误「反馈引导行动」：handler 捕获 core 错误后渲染可读片段，不只返回 500（CLAUDE.md §7）。
- 每个 task 末尾 `make check`（format + lint + test）双绿后 commit；commit message 中文 + Conventional Commits。
- GUI 端点不加 `--json`（职责分离；`--json` 是 CLI 给 AI agent 的契约）。
- 测试里不依赖机器全局 git config；本计划测试不跑 git commit，无此问题。

## File Structure

**core（1 改）**
- `crates/core/src/project.rs` — 加 `pub fn scan_projects` + 单测
- `crates/core/src/lib.rs` — re-export `scan_projects`

**cli（1 改）**
- `crates/cli/src/commands/project.rs` — `Scan` 改调 core，删私有 `scan_projects`

**server handler（2 改）**
- `crates/server/src/routes/skills.rs` — 加 `find` / `install_candidate` / `import` / `upgrade_all` handler；`SkillsMainTpl` 加 `summary` 字段；`render_skills` 加 `summary` 参数
- `crates/server/src/routes/projects.rs` — 加 `add` / `scan` / `rebind` / `apply_profile` handler；`WorkspaceTpl`/`WorkspaceMainTpl` 加 `profiles` 字段

**server 路由（1 改）**
- `crates/server/src/routes/mod.rs` — 注册 8 条新路由

**server 模板（2 创建 + 4 改）**
- Create `crates/server/templates/fragments/find_results.html` — find 候选列表（每条带 install 表单）
- Create `crates/server/templates/fragments/scan_results.html` — scan 目录列表（每条带注册按钮）
- `crates/server/templates/fragments/skills_main.html` — 顶部搜索框 + 候选区容器 + 导入/全升级按钮 + summary 行
- `crates/server/templates/fragments/projects_main.html` — 注册表单 + 扫描表单
- `crates/server/templates/fragments/workspace_main.html` — rebind 表单 + apply-profile 下拉
- `crates/server/templates/project_workspace.html` — 同 workspace_main（页面壳 include 片段，改片段即可，本计划只改片段）

**测试（2 改）**
- `crates/server/tests/common/mod.rs` — 加 `fake_npx` RAII helper（Task 2 引入）
- `crates/server/tests/routes.rs` — 每个端点加集成测试

---

### Task 1: core 下沉 `scan_projects`

**Files:**
- Modify: `crates/core/src/project.rs`（加公共函数 + 单测）
- Modify: `crates/core/src/lib.rs`（re-export）
- Modify: `crates/cli/src/commands/project.rs:71-79`（`Scan` 改调 core）+ 删 `crates/cli/src/commands/project.rs:154-171`（私有 `scan_projects`）

**Interfaces:**
- Produces: `skillkit_core::scan_projects(dir: &Path, depth: u32) -> Result<Vec<PathBuf>>`（后续 Task 7 的 server scan handler 消费）

- [ ] **Step 1: 写失败的单测**

在 `crates/core/src/project.rs` 的 `#[cfg(test)] mod tests` 末尾追加：

```rust
    #[test]
    fn scan_projects_finds_git_dirs_with_depth_limit() {
        let tmp = tempdir().unwrap();
        // tmp/a/.git  → depth 0 也应发现根级 .git
        std::fs::create_dir_all(tmp.path().join("a/.git")).unwrap();
        // tmp/a/b/.git → depth 1 才发现
        std::fs::create_dir_all(tmp.path().join("a/b/.git")).unwrap();
        // tmp/a/b/c/.git → depth 2 才发现
        std::fs::create_dir_all(tmp.path().join("a/b/c/.git")).unwrap();
        // tmp/a/node_modules/.git → 跳过 .git 自身子目录树（不误入）
        std::fs::create_dir_all(tmp.path().join("a/.git/info")).unwrap();

        let d0 = super::scan_projects(&tmp.path().join("a"), 0).unwrap();
        assert_eq!(d0, vec![tmp.path().join("a")], "depth 0 只发现根");

        let d1 = super::scan_projects(&tmp.path().join("a"), 1).unwrap();
        assert!(d1.contains(&tmp.path().join("a")));
        assert!(d1.contains(&tmp.path().join("a/b")));
        assert!(!d1.iter().any(|p| p.ends_with("a/b/c")));

        let d2 = super::scan_projects(&tmp.path().join("a"), 2).unwrap();
        assert!(d2.iter().any(|p| p.ends_with("a/b/c")));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-core scan_projects -- --nocapture`
Expected: 编译失败，`scan_projects` 未定义。

- [ ] **Step 3: 在 core 实现 `scan_projects`**

在 `crates/core/src/project.rs` 的 `pub fn list_ids` 函数之后追加（逻辑照搬 cli 层原实现，仅改 `pub` + 返回 `Result`）：

```rust
/// 扫描目录树，返回含 .git 的项目目录（depth 限制递归深度，跳过 .git 自身子目录）。
pub fn scan_projects(dir: &Path, depth: u32) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if dir.join(".git").exists() {
        found.push(dir.to_path_buf());
    }
    if depth > 0 {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && !p.starts_with(dir.join(".git")) {
                    found.extend(scan_projects(&p, depth - 1)?);
                }
            }
        }
    }
    Ok(found)
}
```

- [ ] **Step 4: re-export**

`crates/core/src/lib.rs:28` 把：
```rust
pub use project::{list_ids as list_project_ids, Project};
```
改为：
```rust
pub use project::{list_ids as list_project_ids, scan_projects, Project};
```

- [ ] **Step 5: cli 改调 core，删私有副本**

`crates/cli/src/commands/project.rs` 的 `ProjectSub::Scan { dir, depth }` 分支：
```rust
        ProjectSub::Scan { dir, depth } => {
            let found = scan_projects(&dir, depth)?;
            if found.is_empty() {
                println!("（未发现项目，project scan 只识别含 .git 的目录）");
            }
            for p in found {
                println!("{}", p.display());
            }
        }
```
改为：
```rust
        ProjectSub::Scan { dir, depth } => {
            let found = skillkit_core::scan_projects(&dir, depth)?;
            if found.is_empty() {
                println!("（未发现项目，project scan 只识别含 .git 的目录）");
            }
            for p in found {
                println!("{}", p.display());
            }
        }
```

删除文件末尾的私有 `fn scan_projects(...)`（原 154-171 行整段）。

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p skillkit-core scan_projects && cargo build -p skillkit-cli`
Expected: core 测试 PASS；cli 编译通过（确认删私有副本后无残留引用）。

- [ ] **Step 7: `make check` 双绿 + commit**

```bash
make check
git add crates/core/src/project.rs crates/core/src/lib.rs crates/cli/src/commands/project.rs
git commit -m "refactor(core): scan_projects 下沉 core——cli 改调 + 单测覆盖 depth/跳过 .git"
```

---

### Task 2: Skills find 端点 + fake npx 测试基建

引入 server 测试共用的 `fake_npx` helper（后续 Task 3/5 复用）。

**Files:**
- Modify: `crates/server/tests/common/mod.rs` — 加 `fake_npx` RAII guard
- Modify: `crates/server/src/routes/skills.rs` — 加 `find` handler + `FindResultsTpl` + `FindQuery`
- Modify: `crates/server/src/routes/mod.rs` — 注册 `GET /skills/find`
- Create: `crates/server/templates/fragments/find_results.html`
- Modify: `crates/server/templates/fragments/skills_main.html` — 顶部搜索框 + 候选区容器
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Produces: `common::fake_npx(skillkit_dir) -> NpxGuard`（Task 3/5 复用）；`skills::find` handler（GET `/skills/find?q=`）；`FindResultsTpl { token, query, candidates }`

- [ ] **Step 1: 写失败的集成测试**

在 `crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn skills_find_renders_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let _g = common::fake_npx(&state.paths);
    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/skills/find?q=pdf")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("owner/repo@pdf"), "应渲染候选 spec");
    assert!(body.contains("https://skills.sh/owner/repo/pdf"), "应渲染 url");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server skills_find_renders_candidates`
Expected: 编译失败（`common::fake_npx` 与 `skills::find` 路由均未定义）。

- [ ] **Step 3: 加 `fake_npx` helper**

在 `crates/server/tests/common/mod.rs` 末尾追加。假 npx 无状态、对 find/add/update 统一响应，因此多测试并发覆盖 PATH 也无害（行为一致 + 各自 cwd 独立）：

```rust
use skillkit_core::Paths;
use std::path::Path;

/// 前置一个假 npx 到 PATH，响应 skills@latest 的 find/add/update。
/// RAII guard：drop 还原 PATH，避免污染其他测试。
pub struct NpxGuard {
    old_path: String,
}

impl Drop for NpxGuard {
    fn drop(&mut self) {
        if self.old_path.is_empty() {
            std::env::remove_var("PATH");
        } else {
            std::env::set_var("PATH", &self.old_path);
        }
    }
}

/// 在 paths.skillkit_dir()/bin 放假 npx，前置 PATH。cwd（skillkit_dir）由 core 的 npx() 设置，
/// 假 npx 在 cwd 写 skills-lock.json / .agents/skills，与真实 npx skills 行为同构。
pub fn fake_npx(paths: &Paths) -> NpxGuard {
    let bin = paths.skillkit_dir().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let sh = bin.join("npx");
    std::fs::write(
        &sh,
        "#!/bin/sh\n\
         if [ \"$1\" = \"skills@latest\" ] && [ \"$2\" = \"find\" ]; then\n\
         \x20 echo \"owner/repo@$3  1K installs  https://skills.sh/owner/repo/$3\"\n\
         \x20 exit 0\n\
         fi\n\
         if [ \"$1\" = \"skills@latest\" ] && [ \"$2\" = \"add\" ]; then\n\
         \x20 skill=\"$5\"\n\
         \x20 mkdir -p \".agents/skills/$skill\"\n\
         \x20 printf -- '---\\nname: %s\\n---\\n# %s\\n' \"$skill\" \"$skill\" > \".agents/skills/$skill/SKILL.md\"\n\
         \x20 printf '{\"skills\":{\"%s\":{\"computedHash\":\"hashnew\"}}}' \"$skill\" > skills-lock.json\n\
         \x20 exit 0\n\
         fi\n\
         if [ \"$1\" = \"skills@latest\" ] && [ \"$2\" = \"update\" ]; then\n\
         \x20 printf '{\"skills\":{\"%s\":{\"computedHash\":\"hashnew\"}}}' \"$3\" > skills-lock.json\n\
         \x20 exit 0\n\
         fi\n\
         exit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let old = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", bin.display(), old));
    NpxGuard { old_path: old }
}
```

- [ ] **Step 4: 加 find handler + 模板结构体**

在 `crates/server/src/routes/skills.rs`：顶部 `use` 补 `Candidate`：
```rust
use skillkit_core::{npx, registry::SkillMeta, Candidate, Registry, Scope, SourcesStore};
```
（若 `npx` 与 `Candidate` 冲突可只加需要的；`npx::find` 用全路径。）

在 `InstallForm` 结构体之前追加查询结构体 + 结果模板：

```rust
#[derive(Deserialize)]
pub struct FindQuery {
    pub q: String,
}

#[derive(Template)]
#[template(path = "fragments/find_results.html")]
pub struct FindResultsTpl<'a> {
    pub token: &'a str,
    pub query: &'a str,
    /// 候选列表，每条带 install 表单。
    pub candidates: Vec<Candidate>,
}

/// find：搜 skills.sh registry，渲染候选片段（每条带 install 按钮）。
pub async fn find(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<FindQuery>,
) -> Response {
    // npx::find 同步阻塞（Command::output），用 spawn_blocking 卸到 blocking 线程池，
    // 避免占用 tokio 工作线程（默认 = CPU 核数）；闭包 move state、clone query。
    let qstr = q.q.clone();
    let result = tokio::task::spawn_blocking(move || npx::find(&state.paths, &qstr)).await;
    match result {
        Ok(Ok(cs)) => {
            let rendered = FindResultsTpl {
                token: &token,
                query: &q.q,
                candidates: cs,
            }
            .render();
            render_str(rendered)
        }
        Ok(Err(e)) => {
            tracing::error!(error = ?e, "find 失败：{}", q.q);
            Html("<p class=\"err\">搜索失败，检查网络/Node 后重试</p>").into_response()
        }
        Err(e) => {
            tracing::error!(error = ?e, "find join 失败：{}", q.q);
            Html("<p class=\"err\">搜索失败，检查网络/Node 后重试</p>").into_response()
        }
    }
}
```

- [ ] **Step 5: 创建 find_results.html 模板**

`crates/server/templates/fragments/find_results.html`：

```html
<div id="find-results">
  {% if candidates.is_empty() %}
  <p class="err">在 skills.sh 未找到「{{ query }}」，换个关键词或检查网络</p>
  {% else %}
  <table class="data">
    <thead><tr><th>候选 (skills.sh)</th><th>ops</th></tr></thead>
    <tbody>
    {% for c in candidates %}
    <tr>
      <td>{{ c.spec }}{% if let Some(u) = c.url %} <a href="{{ u }}" target="_blank">↗</a>{% endif %}</td>
      <td>
        <form class="inline"
              hx-post="/{{ token }}/skills/install-candidate"
              hx-target="body" hx-swap="outerHTML">
          <input type="hidden" name="spec" value="{{ c.spec }}">
          <input type="hidden" name="skill" value="{{ query }}">
          <select name="scope"><option value="local">local</option><option value="global">global</option></select>
          <button>install</button>
        </form>
      </td>
    </tr>
    {% endfor %}
    </tbody>
  </table>
  {% endif %}
</div>
```

- [ ] **Step 6: skills_main.html 加搜索框 + 候选区容器**

`crates/server/templates/fragments/skills_main.html` 在 `<h1>Skills</h1>` 之后、`<table>` 之前插入：

```html
  <div class="find-bar">
    <input type="text" name="q" placeholder="搜 skills.sh 候选（如 pdf）"
           hx-get="/{{ token }}/skills/find"
           hx-trigger="keyup changed delay:400ms"
           hx-target="#find-results" hx-swap="outerHTML"
           hx-indicator="#find-indicator">
    <span id="find-indicator" class="htmx-indicator">搜索中…</span>
    <div id="find-results"></div>
  </div>
```

- [ ] **Step 7: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，在 `.route("/{token}/skills", get(skills::page))` 之后加：
```rust
        .route("/{token}/skills/find", get(skills::find))
```

- [ ] **Step 8: 跑测试确认通过**

Run: `cargo test -p skillkit-server skills_find_renders_candidates`
Expected: PASS（假 npx 响应 find，渲染含 spec + url）。

- [ ] **Step 9: `make check` 双绿 + commit**

```bash
make check
git add crates/server/tests/common/mod.rs crates/server/src/routes/skills.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/find_results.html crates/server/templates/fragments/skills_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Skills find 端点——搜 skills.sh 候选 + fake_npx 测试基建"
```

---

### Task 3: Skills install-candidate 端点

**Files:**
- Modify: `crates/server/src/routes/skills.rs` — `SkillsMainTpl` 加 `summary` 字段；`render_skills` 加 `summary` 参数；现有 page/install/uninstall/upgrade 调用点改传 `None`；加 `install_candidate` handler
- Modify: `crates/server/src/routes/mod.rs` — 注册 `POST /skills/install-candidate`
- Modify: `crates/server/templates/fragments/skills_main.html` — summary 行
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `common::fake_npx`（Task 2）、`FindResultsTpl` 的 install 表单字段（`spec`/`skill`/`scope`）
- Produces: `skills::install_candidate` handler；`SkillsMainTpl.summary: Option<String>`

- [ ] **Step 1: 写失败的集成测试**

`crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn skills_install_candidate_registers_skill() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // 种 skills.sh 源（package=None），install 需要 source 存在
    skillkit_core::SourcesStore::ensure_default(&state.paths).unwrap();
    let _g = common::fake_npx(&state.paths);
    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/install-candidate")
                .header(axum::http::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("spec=owner%2Frepo%40pdf&skill=pdf&scope=local"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let reg = skillkit_core::Registry::load(&state.paths).unwrap();
    let m = reg.get("skills.sh/pdf").expect("应登记 skills.sh/pdf");
    assert_eq!(m.computed_hash.as_deref(), Some("hashnew"));
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server skills_install_candidate_registers_skill`
Expected: 404（路由未注册）。

- [ ] **Step 3: `SkillsMainTpl` 加 summary 字段 + `render_skills` 改造**

`crates/server/src/routes/skills.rs`：

`SkillsMainTpl` 与 `SkillsTpl` 都加 `summary` 字段（写操作返回完整页 body outerHTML 走 SkillsTpl，summary 经 skills.html 的 include 传给 skills_main.html 渲染）。先改 `SkillsMainTpl`：

```rust
#[derive(Template)]
#[template(path = "fragments/skills_main.html")]
pub struct SkillsMainTpl<'a> {
    pub token: &'a str,
    pub skills: Vec<(SkillMeta, String)>,
    pub summary: Option<&'a str>,
}
```

`SkillsTpl` 同样加字段（skills.html 是 `{% extends layout %}{% block content %}{% include "fragments/skills_main.html" %}`，Askama include 共享父模板变量，故 SkillsTpl 的 summary 在 skills_main.html 可见）：

```rust
#[derive(Template)]
#[template(path = "skills.html")]
pub struct SkillsTpl<'a> {
    pub token: &'a str,
    pub skills: Vec<(SkillMeta, String)>,
    pub summary: Option<&'a str>,
}
```

`render_skills` 加 `summary` 参数，两个分支都传：

```rust
fn render_skills(state: AppState, token: String, summary: Option<&str>, fragment: bool) -> Response {
    match Registry::load(&state.paths) {
        Ok(reg) => {
            let skills: Vec<(SkillMeta, String)> = reg
                .skills
                .values()
                .map(|m| (m.clone(), m.id.replace('/', "%2F")))
                .collect();
            let rendered = if fragment {
                SkillsMainTpl { token: &token, skills, summary }.render()
            } else {
                SkillsTpl { token: &token, skills, summary }.render()
            };
            render_str(rendered)
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 registry 失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
```

更新现有调用点（page / install / uninstall / upgrade）：
- `page` → `render_skills(state, token, None, q.is_fragment())`
- `install` 末尾 `render_skills(state, token, None, false)`
- `uninstall` 末尾同上
- `upgrade` 末尾同上

- [ ] **Step 4: skills_main.html 加 summary 行**

`crates/server/templates/fragments/skills_main.html` 在 `<h1>Skills</h1>` 之后加：
```html
  {% if let Some(s) = summary %}<p class="summary">{{ s }}</p>{% endif %}
```

- [ ] **Step 5: 加 install_candidate handler**

`crates/server/src/routes/skills.rs`，在 `install` handler 之后追加：

```rust
#[derive(Deserialize)]
pub struct InstallCandidateForm {
    /// owner/repo@skill，npx skills add 的 package 参数。
    pub spec: String,
    /// skill 名（=find 时的 query），作 canonical 目录名 + registry id 后缀。
    pub skill: String,
    pub scope: Option<String>,
}

/// registry 源（skills.sh）install：find 候选选中后装。source 固定 skills.sh，package 用 spec。
pub async fn install_candidate(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<InstallCandidateForm>,
) -> Response {
    let scope = if matches!(f.scope.as_deref(), Some("global")) {
        Scope::Global
    } else {
        Scope::Local
    };
    match skillkit_core::install(&state.paths, "skills.sh", &f.skill, &f.spec, scope) {
        Ok(_) => render_skills(
            state,
            token,
            Some(&format!("✓ 已安装 skills.sh/{}", f.skill)),
            false,
        ),
        Err(skillkit_core::SkillkitError::SkillAlreadyInstalled { .. }) => {
            Html("<p class=\"err\">该 skill 已安装，可在列表中 upgrade 或 remove</p>").into_response()
        }
        Err(e) => {
            tracing::error!(error = ?e, "install-candidate 失败：{}", f.spec);
            Html("<p class=\"err\">安装失败，检查网络/Node 后重试</p>").into_response()
        }
    }
}
```

- [ ] **Step 6: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，在 find 路由之后加：
```rust
        .route("/{token}/skills/install-candidate", post(skills::install_candidate))
```

- [ ] **Step 7: 跑测试确认通过**

Run: `cargo test -p skillkit-server skills_install_candidate_registers_skill`
Expected: PASS（假 npx add 写 skills-lock.json，registry 登记 skills.sh/pdf，hash=hashnew）。

- [ ] **Step 8: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/skills.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/skills_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Skills registry 源 install——find 候选一键装 + summary 反馈"
```

---

### Task 4: Skills import 端点

**Files:**
- Modify: `crates/server/src/routes/skills.rs` — 加 `import` handler
- Modify: `crates/server/src/routes/mod.rs` — 注册 `POST /skills/import`
- Modify: `crates/server/templates/fragments/skills_main.html` — 导入按钮
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `skillkit_core::import_existing`（已 export）
- Produces: `skills::import` handler

- [ ] **Step 1: 写失败的集成测试**

`crates/server/tests/routes.rs` 末尾追加。import 对 unmanaged 不调 npx，无需 fake_npx：

```rust
#[tokio::test]
async fn skills_import_registers_existing() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // 造存量 skill：~/.agents/skills/foo/SKILL.md
    let foo = state.paths.agents_skills_dir().join("foo");
    std::fs::create_dir_all(&foo).unwrap();
    std::fs::write(foo.join("SKILL.md"), "---\nname: foo\n---\n# foo\n").unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/import")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let reg = skillkit_core::Registry::load(&state.paths).unwrap();
    let m = reg.get("unmanaged/foo").expect("应登记 unmanaged/foo");
    assert!(m.computed_hash.is_none());
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server skills_import_registers_existing`
Expected: 404（路由未注册）。

- [ ] **Step 3: 加 import handler**

`crates/server/src/routes/skills.rs`，在 `upgrade` handler 之后追加：

```rust
/// 导入存量 skill 目录，登记进 registry（无源 → unmanaged）。
pub async fn import(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    match skillkit_core::import_existing(&state.paths, false) {
        Ok(r) => {
            let summary = format!(
                "imported {}，unmanaged {}，reinstalled {}，skipped {}",
                r.imported.len(),
                r.unmanaged.len(),
                r.reinstalled.len(),
                r.skipped.len()
            );
            render_skills(state, token, Some(&summary), false)
        }
        Err(e) => {
            tracing::error!(error = ?e, "import 失败");
            Html("<p class=\"err\">导入失败</p>").into_response()
        }
    }
}
```

- [ ] **Step 4: skills_main.html 加导入按钮**

`crates/server/templates/fragments/skills_main.html`，在 find-bar 之后（或 summary 行之后）加：
```html
  <form class="inline" hx-post="/{{ token }}/skills/import"
        hx-target="body" hx-swap="outerHTML" hx-indicator="#import-indicator">
    <button>导入存量 skill</button>
    <span id="import-indicator" class="htmx-indicator">导入中…</span>
  </form>
```

- [ ] **Step 5: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，在 install-candidate 之后加：
```rust
        .route("/{token}/skills/import", post(skills::import))
```

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p skillkit-server skills_import_registers_existing`
Expected: PASS。

- [ ] **Step 7: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/skills.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/skills_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Skills 导入存量——一键扫描登记 + 计数反馈"
```

---

### Task 5: Skills upgrade-all 端点

**Files:**
- Modify: `crates/server/src/routes/skills.rs` — 加 `upgrade_all` handler
- Modify: `crates/server/src/routes/mod.rs` — 注册 `POST /skills/upgrade-all`
- Modify: `crates/server/templates/fragments/skills_main.html` — 全升级按钮
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `skillkit_core::upgrade_all`（已 export）、`common::fake_npx`
- Produces: `skills::upgrade_all` handler

- [ ] **Step 1: 写失败的集成测试**

`crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn skills_upgrade_all_batch_upgrades() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // 两个 managed skill：dc/ok 无人锁 → 正常升级；dc/conflict 被项目 P1 锁 oldhash → 冲突进 blocked
    let mut reg = skillkit_core::Registry::default();
    for (id, name) in [("dc/ok", "ok"), ("dc/conflict", "conflict")] {
        let canon = state.paths.skillkit_skills_dir().join(name);
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        reg.skills.insert(
            id.into(),
            skillkit_core::registry::SkillMeta {
                id: id.into(),
                name: name.into(),
                source: "dc".into(),
                scope: skillkit_core::Scope::Local,
                version: None,
                computed_hash: Some("oldhash".into()),
                installed_at: "2026-07-31".into(),
                canonical_path: canon.to_string_lossy().into_owned(),
            },
        );
    }
    reg.save(&state.paths).unwrap();

    // P1 锁 dc/conflict=oldhash：upgrade_all(false) 必须把它列进 blocked，而非静默升级
    skillkit_core::Project {
        id: "P1".into(),
        name: "P1".into(),
        path: dir.path().join("p1").to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: [("dc/conflict".to_string(), "oldhash".to_string())]
            .into_iter()
            .collect(),
    }
    .save(&state.paths)
    .unwrap();

    let _g = common::fake_npx(&state.paths);
    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/upgrade-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    let after = skillkit_core::Registry::load(&state.paths).unwrap();
    // dc/ok 无冲突 → 正常升到 hashnew
    assert_eq!(
        after.get("dc/ok").unwrap().computed_hash.as_deref(),
        Some("hashnew"),
        "无冲突的 dc/ok 应升级到 hashnew",
    );
    // dc/conflict 被锁 → 进 blocked 不升级，hash 保持 oldhash（不静默漂移）
    assert_eq!(
        after.get("dc/conflict").unwrap().computed_hash.as_deref(),
        Some("oldhash"),
        "被项目锁定的 dc/conflict 应进 blocked 不升级，hash 不变",
    );
    // summary 反馈冲突 skill + 受影响项目（列出不静默）
    assert!(
        body.contains("dc/conflict") && body.contains("P1"),
        "summary 应列出冲突 skill 与受影响项目：{body}"
    );
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server skills_upgrade_all_batch_upgrades`
Expected: 404。

- [ ] **Step 3: 加 upgrade_all handler**

`crates/server/src/routes/skills.rs`，在 `import` handler 之后追加。GUI 单击「全部升级」对齐 CLI `--all` 默认语义（`yes=false`）：冲突 skill 进 blocked 只列出、不静默升级，避免锁了 oldhash 的项目基线漂移而零反馈。

```rust
/// 全部升级：批量升级 registry 全部 managed skill，冲突进 blocked 列出（不升级）。
pub async fn upgrade_all(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    match skillkit_core::upgrade_all(&state.paths, false) {
        Ok(all) => {
            let mut summary = format!("已升级 {} 个", all.upgraded.len());
            for b in &all.blocked {
                summary.push_str(&format!(
                    "；跳过 {}（影响项目 {}，需重新 apply）",
                    b.id,
                    b.affected.join(", ")
                ));
            }
            render_skills(state, token, Some(&summary), false)
        }
        Err(e) => {
            tracing::error!(error = ?e, "upgrade-all 失败");
            Html("<p class=\"err\">批量升级失败</p>").into_response()
        }
    }
}
```

- [ ] **Step 4: skills_main.html 加全升级按钮**

`crates/server/templates/fragments/skills_main.html`，导入按钮之后加：
```html
  <form class="inline" hx-post="/{{ token }}/skills/upgrade-all"
        hx-target="body" hx-swap="outerHTML" hx-indicator="#upgrade-all-indicator">
    <button>全部升级</button>
    <span id="upgrade-all-indicator" class="htmx-indicator">升级中…</span>
  </form>
```

- [ ] **Step 5: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，import 之后加：
```rust
        .route("/{token}/skills/upgrade-all", post(skills::upgrade_all))
```

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p skillkit-server skills_upgrade_all_batch_upgrades`
Expected: PASS。

- [ ] **Step 7: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/skills.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/skills_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Skills 全部升级——批量 + 冲突项目反馈"
```

---

### Task 6: Projects add 端点

**Files:**
- Modify: `crates/server/src/routes/projects.rs` — 加 `add` handler + `ProjectAddForm`
- Modify: `crates/server/src/routes/mod.rs` — 注册 `POST /projects`
- Modify: `crates/server/templates/fragments/projects_main.html` — 注册表单
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `skillkit_core::Project::register` + `Config::load`（agents 默认，照 `cli/commands/project.rs:55-60`）
- Produces: `projects::add` handler

- [ ] **Step 1: 写失败的集成测试**

`crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn projects_add_registers_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let proj_root = dir.path().join("myapp");
    std::fs::create_dir_all(&proj_root).unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects")
                .header(axum::http::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "path={}",
                    urlencode(proj_root.to_string_lossy())
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ids = skillkit_core::list_project_ids(&state.paths).unwrap();
    assert_eq!(ids.len(), 1, "应注册 1 个项目");
    let proj = skillkit_core::Project::load(&state.paths, &ids[0]).unwrap();
    assert!(proj.path.contains("myapp"));
}
```

测试顶部（`routes.rs` 的 `use` 之后）需要一个 urlencoded 辅助。`projects.rs` 已用 `form_urlencoded` 手动解析重复 key，但这里单字段 path 直接内联编码即可。在测试文件顶部 `mod common;` 下方加 helper：

```rust
fn urlencode(s: &str) -> String {
    s.replace('/', "%2F")
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server projects_add_registers_and_persists`
Expected: 失败（`POST /projects` 当前只有 GET）。

- [ ] **Step 3: 加 add handler + form 结构体**

`crates/server/src/routes/projects.rs` 顶部 `use` 补 `Config`、`PathBuf`：
```rust
use skillkit_core::{
    build_status, compute_diff, run_apply, scan_shared, ApplyDiff, ApplyReport, Config, Project,
    Registry, SkillMeta, StatusView,
};
use std::path::{Path as StdPath, PathBuf};
```

在 `ApplyResultTpl` 结构体之后追加 form + handler：

```rust
#[derive(Deserialize)]
pub struct ProjectAddForm {
    pub path: String,
    /// 可选，逗号分隔；留空用 config 全 agent。
    pub agents: Option<String>,
}

/// 注册新项目：canonicalize path → Project::register → save → 刷新列表。
pub async fn add(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<ProjectAddForm>,
) -> Response {
    let abs = PathBuf::from(&f.path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&f.path));
    let agents = match f.agents.as_deref() {
        Some(a) if !a.trim().is_empty() => a
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>(),
        _ => Config::load(&state.paths)
            .map(|c| c.agents.iter().map(|a| a.name.clone()).collect())
            .unwrap_or_default(),
    };
    let proj = Project::register(abs, agents);
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let mut projects = Vec::new();
    if let Ok(ids) = skillkit_core::list_project_ids(&state.paths) {
        for id in ids {
            if let Ok(p) = Project::load(&state.paths, &id) {
                projects.push(p);
            }
        }
    }
    render_list(token, projects, false)
}
```

- [ ] **Step 4: projects_main.html 加注册表单**

`crates/server/templates/fragments/projects_main.html`（在 `<h1>` 之后、项目列表之前）加：
```html
  <form class="inline" hx-post="/{{ token }}/projects"
        hx-target="body" hx-swap="outerHTML">
    <input type="text" name="path" placeholder="项目绝对路径（如 /Users/me/app）" required>
    <input type="text" name="agents" placeholder="agents（可选，逗号分隔，留空用全部）">
    <button>注册项目</button>
  </form>
```

- [ ] **Step 5: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，把：
```rust
        .route("/{token}/projects", get(projects::list))
```
改为：
```rust
        .route("/{token}/projects", get(projects::list).post(projects::add))
```

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p skillkit-server projects_add_registers_and_persists`
Expected: PASS。

- [ ] **Step 7: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/projects.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/projects_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Projects 注册项目——列表页表单 + 持久化"
```

---

### Task 7: Projects scan 端点

**Files:**
- Modify: `crates/server/src/routes/projects.rs` — 加 `scan` handler + `ScanForm` + `ScanResultsTpl`
- Modify: `crates/server/src/routes/mod.rs` — 注册 `POST /projects/scan`
- Create: `crates/server/templates/fragments/scan_results.html`
- Modify: `crates/server/templates/fragments/projects_main.html` — 扫描表单
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `skillkit_core::scan_projects`（Task 1）
- Produces: `projects::scan` handler；`ScanResultsTpl { token, dirs }`

- [ ] **Step 1: 写失败的集成测试**

`crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn projects_scan_finds_git_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let root = dir.path().join("scanroot");
    std::fs::create_dir_all(root.join("proj1/.git")).unwrap();
    std::fs::create_dir_all(root.join("proj2/.git")).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/scan")
                .header(axum::http::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "dir={}&depth=2",
                    urlencode(&root.to_string_lossy())
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("proj1"), "scan 结果含 proj1");
    assert!(body.contains("proj2"), "scan 结果含 proj2");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server projects_scan_finds_git_dirs`
Expected: 404。

- [ ] **Step 3: 加 scan handler + 模板结构体**

`crates/server/src/routes/projects.rs`，在 `ProjectAddForm` 之后追加：

```rust
#[derive(Deserialize)]
pub struct ScanForm {
    pub dir: String,
    pub depth: Option<u32>,
}

#[derive(Template)]
#[template(path = "fragments/scan_results.html")]
pub struct ScanResultsTpl<'a> {
    pub token: &'a str,
    pub dirs: Vec<String>,
}

/// 扫描目录发现项目，渲染候选（每条带注册按钮，复用 POST /projects）。
pub async fn scan(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<ScanForm>,
) -> Response {
    let depth = f.depth.unwrap_or(3);
    match skillkit_core::scan_projects(StdPath::new(&f.dir), depth) {
        Ok(dirs) => {
            let dirs: Vec<String> = dirs
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            let rendered = ScanResultsTpl {
                token: &token,
                dirs,
            }
            .render();
            render_str_html(rendered)
        }
        Err(e) => {
            tracing::error!(error = ?e, "scan 失败：{}", f.dir);
            Html("<p class=\"err\">扫描失败，检查目录路径</p>").into_response()
        }
    }
}
```

注意：`projects.rs` 已有一个私有 `fn render_str(rendered: askama::Result<String>) -> Response`。这里新增的 scan 复用它即可——把上面 `render_str_html(rendered)` 改为复用现有 `render_str(rendered)`（`ScanResultsTpl.render()` 返回 `askama::Result<String>`，签名匹配）。即 handler 末尾用：
```rust
            render_str(rendered)
```
（删去 `render_str_html` 命名，直接调现有 `render_str`。）

- [ ] **Step 4: 创建 scan_results.html 模板**

`crates/server/templates/fragments/scan_results.html`：

```html
<div id="scan-results">
  {% if dirs.is_empty() %}
  <p class="err">未发现含 .git 的项目目录</p>
  {% else %}
  <table class="data">
    <thead><tr><th>发现的项目目录</th><th>ops</th></tr></thead>
    <tbody>
    {% for d in dirs %}
    <tr>
      <td>{{ d }}</td>
      <td>
        <form class="inline" hx-post="/{{ token }}/projects"
              hx-target="body" hx-swap="outerHTML">
          <input type="hidden" name="path" value="{{ d }}">
          <button>注册</button>
        </form>
      </td>
    </tr>
    {% endfor %}
    </tbody>
  </table>
  {% endif %}
</div>
```

- [ ] **Step 5: projects_main.html 加扫描表单**

`crates/server/templates/fragments/projects_main.html`，注册表单之后加：
```html
  <form class="inline" hx-post="/{{ token }}/projects/scan"
        hx-target="#scan-results" hx-swap="outerHTML"
        hx-indicator="#scan-indicator">
    <input type="text" name="dir" placeholder="扫描根目录（如 ~/code）" required>
    <input type="number" name="depth" value="3" min="0" max="5">
    <button>扫描发现</button>
    <span id="scan-indicator" class="htmx-indicator">扫描中…</span>
  </form>
  <div id="scan-results"></div>
```

- [ ] **Step 6: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，在 `projects` 路由之后加：
```rust
        .route("/{token}/projects/scan", post(projects::scan))
```

- [ ] **Step 7: 跑测试确认通过**

Run: `cargo test -p skillkit-server projects_scan_finds_git_dirs`
Expected: PASS。

- [ ] **Step 8: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/projects.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/scan_results.html crates/server/templates/fragments/projects_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Projects 扫描发现——core scan_projects + 结果勾选注册"
```

---

### Task 8: Projects rebind 端点

**Files:**
- Modify: `crates/server/src/routes/projects.rs` — 加 `rebind` handler + `RebindForm`
- Modify: `crates/server/src/routes/mod.rs` — 注册 `POST /projects/{id}/rebind`
- Modify: `crates/server/templates/fragments/workspace_main.html` — rebind 表单
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `Project::rebind`（已存在）
- Produces: `projects::rebind` handler

- [ ] **Step 1: 写失败的集成测试**

`crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn projects_rebind_updates_path() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let old = dir.path().join("old-name");
    std::fs::create_dir_all(&old).unwrap();
    let new = dir.path().join("new-name");
    std::fs::create_dir_all(&new).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "old-name".into(),
        path: old.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/rebind")
                .header(axum::http::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "path={}",
                    urlencode(&new.to_string_lossy())
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let after = skillkit_core::Project::load(&state.paths, "ABCDEF12").unwrap();
    assert_eq!(after.id, "ABCDEF12", "rebind 不变 id");
    assert_eq!(after.name, "new-name");
    assert!(after.path.contains("new-name"));
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server projects_rebind_updates_path`
Expected: 404。

- [ ] **Step 3: 加 rebind handler**

`crates/server/src/routes/projects.rs`，在 `scan` handler 之后追加：

```rust
#[derive(Deserialize)]
pub struct RebindForm {
    pub path: String,
}

/// 重绑定：项目移动/改名后更新 path/name，id 不变。
pub async fn rebind(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
    Form(f): Form<RebindForm>,
) -> Response {
    let Ok(mut proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proj.rebind(StdPath::new(&f.path));
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_workspace(state, token, proj, false)
}
```

- [ ] **Step 4: workspace_main.html 加 rebind 表单**

`crates/server/templates/fragments/workspace_main.html`，在工作台内容末尾（或 status 区之后）加：
```html
  <details>
    <summary>重绑定（项目移动/改名后修正路径）</summary>
    <form class="inline" hx-post="/{{ token }}/projects/{{ project.id }}/rebind"
          hx-target="body" hx-swap="outerHTML">
      <input type="text" name="path" placeholder="新路径" required>
      <button>重绑定</button>
    </form>
  </details>
```

- [ ] **Step 5: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，在 `projects/{id}` 相关路由处加（紧邻 `/{token}/projects/{id}/skills`）：
```rust
        .route("/{token}/projects/{id}/rebind", post(projects::rebind))
```

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p skillkit-server projects_rebind_updates_path`
Expected: PASS。

- [ ] **Step 7: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/projects.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/workspace_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Projects 重绑定——移动/改名后修正 path，id 不变"
```

---

### Task 9: Projects apply-profile 端点

**Files:**
- Modify: `crates/server/src/routes/projects.rs` — `WorkspaceTpl`/`WorkspaceMainTpl` 加 `profiles` 字段；`render_workspace` 注入 profile 名列表；加 `apply_profile` handler + `ApplyProfileForm`
- Modify: `crates/server/src/routes/mod.rs` — 注册 `POST /projects/{id}/apply-profile`
- Modify: `crates/server/templates/fragments/workspace_main.html` — apply-profile 下拉
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `skillkit_core::Profile::load` + `Project::apply_profile` + `list_profile_names`
- Produces: `projects::apply_profile` handler；`WorkspaceTpl.profiles: Vec<String>`

- [ ] **Step 1: 写失败的集成测试**

`crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn projects_apply_profile_merges_skills() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let proj_root = dir.path().join("p");
    std::fs::create_dir_all(&proj_root).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "p".into(),
        path: proj_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();
    skillkit_core::Profile {
        name: "fe".into(),
        description: String::new(),
        skills: vec!["dc/logseq".into(), "dc/dataviz".into()],
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/apply-profile")
                .header(axum::http::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("profile=fe"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // 防 P0 回归：handler 必须只返回 status 片段（含 #status-panel），不能返回整页 workspace。
    // 整页才含 installed_skills 标题；配合模板 hx-target="#status-panel" 才不会整页替换。
    let body = common::body_string(resp).await;
    assert!(body.contains("status-panel"), "apply-profile 响应应含 status 片段");
    assert!(!body.contains("installed_skills"), "apply-profile 不应返回整页 workspace");
    let after = skillkit_core::Project::load(&state.paths, "ABCDEF12").unwrap();
    assert_eq!(after.installed_skills, vec!["dc/logseq".to_string(), "dc/dataviz".to_string()]);
    assert!(after.applied_profiles.contains(&"fe".to_string()));
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server projects_apply_profile_merges_skills`
Expected: 404。

- [ ] **Step 3: Workspace 模板加 profiles 字段 + render_workspace 注入**

`crates/server/src/routes/projects.rs`：

`WorkspaceTpl` 加字段：
```rust
#[derive(Template)]
#[template(path = "project_workspace.html")]
pub struct WorkspaceTpl<'a> {
    pub token: &'a str,
    pub project: &'a Project,
    pub status: StatusView,
    pub shared: Vec<String>,
    pub all_skills: Vec<(SkillMeta, bool)>,
    pub profiles: Vec<String>,
}
```
`WorkspaceMainTpl` 同样加 `pub profiles: Vec<String>`。

`render_workspace` 内，在 `let shared = ...` 之后加：
```rust
    let profiles = skillkit_core::list_profile_names(&state.paths).unwrap_or_default();
```
两个分支构造模板处都加 `profiles`（与 `shared` 同级）：
```rust
        WorkspaceMainTpl { token: &token, project: &proj, status, shared, all_skills, profiles }.render()
        // ...
        WorkspaceTpl { token: &token, project: &proj, status, shared, all_skills, profiles }.render()
```

- [ ] **Step 4: 加 apply_profile handler**

`crates/server/src/routes/projects.rs`，在 `rebind` handler 之后追加：

```rust
#[derive(Deserialize)]
pub struct ApplyProfileForm {
    pub profile: String,
}

/// 应用 profile：把 profile 的 skills 灌入 installed_skills，刷新 status 片段。
pub async fn apply_profile(
    State(state): State<AppState>,
    Path((_token, id)): Path<(String, String)>,
    Form(f): Form<ApplyProfileForm>,
) -> Response {
    let Ok(mut proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let profile = match skillkit_core::Profile::load(&state.paths, &f.profile) {
        Ok(p) => p,
        Err(_) => return Html("<p class=\"err\">profile 不存在</p>").into_response(),
    };
    proj.apply_profile(&f.profile, &profile.skills);
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    status_fragment(state, proj)
}
```

- [ ] **Step 5: workspace_main.html 加 apply-profile 下拉**

`crates/server/templates/fragments/workspace_main.html`，rebind 表单之后加。target 用 `#status-panel`（非 body）：handler 返回 status 片段，对齐本文件 set_skills/apply 既有模式（`:8`/`:14`），否则整页 body 会被替换成一行 status 面板：
```html
  <form class="inline" hx-post="/{{ token }}/projects/{{ project.id }}/apply-profile"
        hx-target="#status-panel" hx-swap="outerHTML">
    <select name="profile">
      <option value="">（选择 profile）</option>
      {% for p in profiles %}<option value="{{ p }}">{{ p }}</option>{% endfor %}
    </select>
    <button>应用 profile</button>
  </form>
```

- [ ] **Step 6: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，rebind 路由之后加：
```rust
        .route("/{token}/projects/{id}/apply-profile", post(projects::apply_profile))
```

- [ ] **Step 7: 跑测试确认通过**

Run: `cargo test -p skillkit-server projects_apply_profile_merges_skills`
Expected: PASS。

- [ ] **Step 8: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/projects.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/workspace_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Projects 应用 profile——下拉灌入 installed_skills + status 刷新"
```

---

### Task 10: 收尾 — 全量验证 + README 同步

**Files:**
- Modify: `README.md`（serve 段补 GUI 已对齐 CLI）
- Modify: `docs/sessions/`（按项目惯例追加交接，可选）

- [ ] **Step 1: 全量 `make check`**

Run: `make check`
Expected: format + clippy(-D warnings) + 全量测试双绿。若 clippy 报未使用 import（如 skills.rs 的 `Candidate` 若 find handler 没真用到），按警告删除多余 import。

- [ ] **Step 2: 验证回归 — 现有 fragment 契约测试仍过**

Run: `cargo test -p skillkit-server fragment_response_is_main_content_only`
Expected: PASS（确认 skills_main.html / projects_main.html / workspace_main.html 改动后 `?fragment=1` 仍不含 nav）。

- [ ] **Step 3: README 同步 GUI 能力**

`README.md` 的 `### serve — Web GUI` 段，把：
```bash
skillkit serve [--port 7317] [--no-open]          # 四视图 + apply 闭环 + SSE
```
说明改为反映 GUI 已对齐 CLI 全部操作，例如：
```bash
skillkit serve [--port 7317] [--no-open]          # 四视图，覆盖 CLI 全部操作（find/装/卸/升级/导入、project 注册/扫描/重绑/apply-profile/apply 闭环）+ SSE
```

- [ ] **Step 4: （可选）真实浏览器 e2e 冒烟**

Run: `make e2e`
Expected: 现有 playwright e2e 通过（若 e2e 脚本断言旧 UI 结构，按需更新选择器）。若 e2e 不覆盖新端点，跳过本步不影响收尾。

- [ ] **Step 5: commit**

```bash
git add README.md
git commit -m "docs: serve 段同步——GUI 已对齐 CLI 全部操作"
```

---

## Self-Review 结论

**Spec 覆盖**：8 条缺口逐一对应 Task 2-9（find/install/import/upgrade-all/add/scan/rebind/apply-profile），scan 下沉对应 Task 1，README 同步对应 Task 10。无遗漏。

**占位扫描**：无 TBD/TODO；每个端点 task 含完整 handler/form/模板/测试代码。

**类型一致性**：`scan_projects(dir:&Path, depth:u32)->Result<Vec<PathBuf>>`（Task 1 定义，Task 7 消费，签名一致）；`SkillsMainTpl.summary: Option<&str>`（Task 3 定义，Task 4/5 的 `render_skills(..., Some(&summary), false)` 调用一致）；`WorkspaceTpl.profiles: Vec<String>`（Task 9 定义+注入一致）；`common::fake_npx`（Task 2 定义，Task 3/5 消费）；`render_skills` 签名 `(state, token, summary, fragment)`（Task 3 改造，Task 2 的 page 调用点同步更新——注意 Task 2 的 find handler 不调 render_skills，无影响；Task 2 之前 page/install/uninstall/upgrade 仍用旧签名，Task 3 统一改造）。

**跨 task 依赖顺序**：Task 1 → Task 7（scan_projects）；Task 2 → Task 3/5（fake_npx）；Task 3 → Task 4/5（render_skills.summary 签名）；Task 9 自洽。建议按 Task 1→2→3→4→5→6→7→8→9→10 顺序执行。
