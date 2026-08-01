# 项目路径文件选择向导 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Projects 视图「注册项目」「扫描发现」两个表单加文件选择向导——输入框旁加「浏览...」按钮，点开逐级目录浏览面板，选定后回填输入框；输入框仍可手输（混合形态）。

**Architecture:** 1 个新端点 `GET /{token}/projects/browse`（server 薄壳调 `std::fs` 读目录 + 过滤），2 个 Askama 片段（浏览面板 + 选定 oob 回填），projects_main.html 两表单各加「浏览...」按钮 + 面板 div。选定回填用 htmx 原生 `hx-swap-oob`，零裸 JS。

**Tech Stack:** Rust 2021 + Axum + Askama + htmx（`hx-swap-oob`）+ `tempfile` 集成测试。

## Global Constraints

- 路径绝不硬编码：home 兜底用 `dirs::home_dir()`，不写死 `/Users/...`（CLAUDE.md §7）。
- server 薄壳：browse handler 只做 `std::fs` 读目录 + 过滤，零业务逻辑泄漏（CLAUDE.md §5）。
- 前端强规则（§7.5）：片段外层固定 id（浏览面板外层 id = `panel` 参数，与 hx-target 一致，替换后 id 不丢）。
- 错误「反馈引导行动」：路径不可读给可读提示片段，不 panic（CLAUDE.md §7）。
- **input id 与 name 一致约定**：注册 `id=path name=path`、扫描 `id=dir name=dir`——让 browse 选定的 `hx-swap-oob` 回填无需属性映射（oob 是 outerHTML 替换，重建 input 时 id=name 一致最简）。
- 改完每个 task 跑 `make check`（format + lint + test）双绿后 commit；commit message 中文 + Conventional Commits。

## File Structure

**server（2 改 + 2 创建）**
- Modify: `crates/server/src/routes/projects.rs` — 加 `browse` handler + `BrowseQuery` + 辅助（`resolve_dir` / `list_subdirs` / `parent_of`）+ `BrowseTpl` / `BrowseSelectTpl` 结构体
- Modify: `crates/server/src/routes/mod.rs` — 注册 `GET /{token}/projects/browse`
- Create: `crates/server/templates/fragments/browse.html` — 浏览面板（当前路径 + 上级 + 子目录列表的进入/选定按钮）
- Create: `crates/server/templates/fragments/browse_select.html` — 选定的 oob 回填片段（input + 清空 panel）
- Modify: `crates/server/templates/fragments/projects_main.html` — 注册/扫描两表单各加「浏览...」按钮 + 面板 div；input 改 id=path/dir（=name）
- Modify: `crates/server/Cargo.toml` — 加 `dirs` 依赖（home 兜底用）

**测试（1 改）**
- Modify: `crates/server/tests/routes.rs` — browse 端点 3 用例 + projects_main 渲染 1 用例

---

### Task 1: browse 端点 + 浏览/选定片段

**Files:**
- Modify: `crates/server/Cargo.toml`（加 `dirs`）
- Modify: `crates/server/src/routes/projects.rs`（加 handler + 辅助 + 模板结构体）
- Modify: `crates/server/src/routes/mod.rs`（注册路由）
- Create: `crates/server/templates/fragments/browse.html`
- Create: `crates/server/templates/fragments/browse_select.html`
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Produces: `projects::browse` handler（GET `/{token}/projects/browse`，query: `path`/`into`/`panel`/`select`）；`BrowseQuery{path:Option<String>, into:String, panel:String, select:Option<String>}`

- [ ] **Step 1: 加 `dirs` 依赖**

`crates/server/Cargo.toml` 的 `[dependencies]` 加（与 core 同版本，从 `crates/core/Cargo.toml` 抄版本号）：
```toml
dirs = "<与 core 一致的版本>"
```

- [ ] **Step 2: 写失败的集成测试（3 用例）**

`crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn projects_browse_lists_subdirs_skips_hidden_and_files() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    std::fs::create_dir_all(dir.path().join("b")).unwrap();
    std::fs::create_dir_all(dir.path().join(".hidden")).unwrap();
    std::fs::write(dir.path().join("file.txt"), "x").unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(&format!(
                    "/test-token/projects/browse?path={}&into=path&panel=browse-panel-add",
                    dir.path().display()
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("a/"), "应含子目录 a");
    assert!(body.contains("b/"), "应含子目录 b");
    assert!(!body.contains(".hidden"), "跳过隐藏目录");
    assert!(!body.contains("file.txt"), "跳过文件");
    assert!(body.contains("进入"), "每条有进入按钮");
    assert!(body.contains("选定"), "每条有选定按钮");
    assert!(body.contains("上级"), "有上级按钮");
}

#[tokio::test]
async fn projects_browse_select_returns_oob_to_fill_input() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    let base = dir.path().display().to_string();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(&format!(
                    "/test-token/projects/browse?path={base}&select=a&into=path&panel=browse-panel-add"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    // oob 回填：input 带 id=name=path + value=选定路径 + hx-swap-oob
    assert!(body.contains(r#"id="path""#), "oob input id=path");
    assert!(body.contains(r#"name="path""#), "oob input name=path 保留（提交用）");
    assert!(
        body.contains(&format!("{base}/a")),
        "input value 是选定绝对路径"
    );
    assert!(body.contains(r#"hx-swap-oob="true""#), "oob 标记");
    // oob 清空面板
    assert!(
        body.contains(r#"id="browse-panel-add""#),
        "含 panel oob（清空关闭）"
    );
}

#[tokio::test]
async fn projects_browse_unreadable_path_returns_hint() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/projects/browse?path=/nonexistent-skillkit-xyz-123&into=path&panel=browse-panel-add")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("不可读"), "不可读路径给可读提示，不 panic");
}
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -p skillkit-server projects_browse 2>&1 | tail -20`
Expected: 编译失败（`projects::browse` 与路由均未定义）。

- [ ] **Step 4: 加 browse handler + 辅助 + 模板结构体**

`crates/server/src/routes/projects.rs`，顶部 `use` 补 `PathBuf`（已用 `Path as StdPath, PathBuf`，确认在）。在文件末尾的 `fn render_str` 之前追加：

```rust
#[derive(Deserialize)]
pub struct BrowseQuery {
    /// 要列的目录（空/无效 → home）。
    pub path: Option<String>,
    /// 选定时回填的输入框 id（= name，如 path / dir）。
    pub into: String,
    /// 浏览面板 div id（如 browse-panel-add）。
    pub panel: String,
    /// 存在时表示「选定 path 下此子目录名」，触发 oob 回填。
    pub select: Option<String>,
}

/// 目录浏览：列 path 下子目录（跳过隐藏/文件），或带 select 时返回 hx-swap-oob 回填输入框。
pub async fn browse(
    Path(token): Path<String>,
    Query(q): Query<BrowseQuery>,
) -> Response {
    let base = resolve_dir(q.path.as_deref());
    // 选定动作：oob 回填 input + 清空面板
    if let Some(name) = &q.select {
        let full = base.join(name);
        let rendered = BrowseSelectTpl {
            into: &q.into,
            panel: &q.panel,
            value: &full.to_string_lossy(),
        }
        .render();
        return render_str(rendered);
    }
    // 浏览动作：列子目录
    match list_subdirs(&base) {
        Ok(dirs) => {
            let parent = parent_of(&base).to_string_lossy().into_owned();
            let rendered = BrowseTpl {
                token: &token,
                current: &base.to_string_lossy(),
                parent: &parent,
                into: &q.into,
                panel: &q.panel,
                dirs,
            }
            .render();
            render_str(rendered)
        }
        Err(e) => {
            tracing::warn!(error = ?e, "browse 不可读：{}", base.display());
            Html("<p class=\"err\">目录不可读，检查路径或权限</p>").into_response()
        }
    }
}

/// 解析路径：空 → home；`~` 开头 → home + rest；否则 canonicalize（失败用原值，不 panic）。
fn resolve_dir(raw: Option<&str>) -> PathBuf {
    let raw = raw.map(str::trim).unwrap_or_default();
    if raw.is_empty() {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    }
    if let Some(rest) = raw.strip_prefix('~') {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        return home.join(rest.trim_start_matches('/'));
    }
    PathBuf::from(raw)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(raw))
}

/// 列子目录（跳过隐藏 `.` 开头 + 跳过文件），按名字排序。
fn list_subdirs(dir: &StdPath) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() && !name.starts_with('.') {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// 父目录；根的父是自身（模板里 parent==current 时不渲染上级按钮）。
fn parent_of(dir: &StdPath) -> PathBuf {
    dir.parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.to_path_buf())
}
```

在 `ApplyResultTpl` 结构体之后追加两个模板结构体：

```rust
#[derive(Template)]
#[template(path = "fragments/browse.html")]
pub struct BrowseTpl<'a> {
    pub token: &'a str,
    pub current: &'a str,
    pub parent: &'a str,
    pub into: &'a str,
    pub panel: &'a str,
    pub dirs: Vec<String>,
}

#[derive(Template)]
#[template(path = "fragments/browse_select.html")]
pub struct BrowseSelectTpl<'a> {
    pub into: &'a str,
    pub panel: &'a str,
    pub value: &'a str,
}
```

- [ ] **Step 5: 创建 browse.html 模板**

`crates/server/templates/fragments/browse.html`：

```html
<div id="{{ panel }}" class="browse-panel">
  <div class="browse-cwd">📁 {{ current }}
    {% if parent != current %}
    <button type="button"
            hx-get="/{{ token }}/projects/browse?path={{ parent }}&into={{ into }}&panel={{ panel }}"
            hx-target="#{{ panel }}">↑ 上级</button>
    {% endif %}
  </div>
  {% if dirs.is_empty() %}
  <p class="muted">（无子目录）</p>
  {% else %}
  <ul class="browse-list">
    {% for d in dirs %}
    <li>
      <span>{{ d }}/</span>
      <button type="button"
              hx-get="/{{ token }}/projects/browse?path={{ current }}/{{ d }}&into={{ into }}&panel={{ panel }}"
              hx-target="#{{ panel }}">进入</button>
      <button type="button"
              hx-get="/{{ token }}/projects/browse?path={{ current }}&select={{ d }}&into={{ into }}&panel={{ panel }}"
              hx-swap="none">✓ 选定</button>
    </li>
    {% endfor %}
  </ul>
  {% endif %}
</div>
```

- [ ] **Step 6: 创建 browse_select.html 模板**

`crates/server/templates/fragments/browse_select.html`（选定 oob 回填——input 重建带 id/name/value，panel 清空）：

```html
<input id="{{ into }}" name="{{ into }}" type="text" required value="{{ value }}" hx-swap-oob="true">
<div id="{{ panel }}" hx-swap-oob="true"></div>
```

- [ ] **Step 7: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，在 `/{token}/projects/scan` 之后加：
```rust
        .route("/{token}/projects/browse", get(projects::browse))
```

- [ ] **Step 8: 跑测试确认通过**

Run: `cargo test -p skillkit-server projects_browse 2>&1 | tail -15`
Expected: 3 个 browse 测试全 PASS。

- [ ] **Step 9: `make check` 双绿 + commit**

```bash
make check
git add crates/server/Cargo.toml crates/server/src/routes/projects.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/browse.html crates/server/templates/fragments/browse_select.html crates/server/tests/routes.rs
git commit -m "feat(gui): Projects 路径浏览端点——逐级目录 + hx-swap-oob 选定回填"
```

---

### Task 2: 前端接入（projects_main.html 两表单）

**Files:**
- Modify: `crates/server/templates/fragments/projects_main.html`
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: Task 1 的 `browse` 端点（`GET /{token}/projects/browse?into=...&panel=...`）+ 约定 input id=name（`path` / `dir`）

- [ ] **Step 1: 写失败的渲染测试**

`crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn projects_main_renders_browse_buttons_and_panels() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    // 注册表单：input id=path + 浏览按钮 + panel
    assert!(body.contains(r#"id="path""#), "注册 input id=path");
    assert!(
        body.contains("/projects/browse?into=path&panel=browse-panel-add"),
        "注册浏览按钮调 browse"
    );
    assert!(body.contains(r#"id="browse-panel-add""#), "注册面板 div");
    // 扫描表单：input id=dir + 浏览按钮 + panel
    assert!(body.contains(r#"id="dir""#), "扫描 input id=dir");
    assert!(
        body.contains("/projects/browse?into=dir&panel=browse-panel-scan"),
        "扫描浏览按钮调 browse"
    );
    assert!(body.contains(r#"id="browse-panel-scan""#), "扫描面板 div");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server projects_main_renders_browse 2>&1 | tail -15`
Expected: FAIL（projects_main 当前 input id 不是 path/dir，无浏览按钮/面板）。

- [ ] **Step 3: 改 projects_main.html**

`crates/server/templates/fragments/projects_main.html` 把现有「注册表单 + 扫描表单」段（input id 改为 path/dir，各加浏览按钮 + 表单后加面板 div）。替换从 `<h1>Projects</h1>` 之后到 `<ul>`（项目列表）之前的两个表单段为：

```html
<h1>Projects</h1>
  <form class="inline" hx-post="/{{ token }}/projects"
        hx-target="body" hx-swap="outerHTML">
    <input id="path" name="path" type="text" placeholder="项目绝对路径（如 /Users/me/app）" required>
    <button type="button"
            hx-get="/{{ token }}/projects/browse?into=path&panel=browse-panel-add"
            hx-target="#browse-panel-add"
            hx-include="#path">浏览...</button>
    <input name="agents" placeholder="agents（可选，逗号分隔，留空用全部）">
    <button>注册项目</button>
  </form>
  <div id="browse-panel-add"></div>
  <form class="inline" hx-post="/{{ token }}/projects/scan"
        hx-target="#scan-results" hx-swap="outerHTML"
        hx-indicator="#scan-indicator">
    <input id="dir" name="dir" type="text" placeholder="扫描根目录（如 ~/code）" required>
    <button type="button"
            hx-get="/{{ token }}/projects/browse?into=dir&panel=browse-panel-scan"
            hx-target="#browse-panel-scan"
            hx-include="#dir">浏览...</button>
    <input type="number" name="depth" value="3" min="0" max="5">
    <button>扫描发现</button>
    <span id="scan-indicator" class="htmx-indicator">扫描中…</span>
  </form>
  <div id="browse-panel-scan"></div>
  <div id="scan-results"></div>
  <ul>
```

注意：
- input `id=path name=path`、`id=dir name=dir`（id=name 约定，Task 1 oob 重建依赖）。
- 浏览按钮 `type="button"`（不触发表单提交）+ `hx-include="#path"`（带输入框当前值作 browse 的 `path` 起点；输入框 name=path，htmx 把它加进 browse query）。
- 面板 div 在各表单之后（就近展开，方案 B）。
- 扫描表单原有 `#scan-results` + `#scan-indicator` 保留，新增 `#browse-panel-scan`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p skillkit-server projects_main_renders_browse 2>&1 | tail -15`
Expected: PASS。

- [ ] **Step 5: `make check` + `make e2e` 回归 + commit**

```bash
make check
make e2e
git add crates/server/templates/fragments/projects_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Projects 表单接浏览向导——输入框+浏览按钮+就近面板"
```

Expected: `make check` 双绿；`make e2e` 现有 6 用例过（projects_main 改动不应破坏；若 e2e 选择器因 input id 变化失败，按需更新 `e2e/test_ui.py`）。手动走查可选：`make run ARGS="serve"`，注册/扫描表单点「浏览...」逐级选目录、选定回填。

---

## Self-Review 结论

**Spec 覆盖**：spec §3.1 端点 → Task 1 handler；§3.2 browse 片段 → Task 1 browse.html；§3.3 前端改造 → Task 2 projects_main.html；§3.4 选定 oob → Task 1 browse_select.html + handler select 分支；§3.5 路径规则（~ 展开/canonicalize/home 兜底/跳过隐藏）→ Task 1 resolve_dir/list_subdirs；§5 测试 → Task 1 三用例 + Task 2 渲染用例。无遗漏。

**占位扫描**：无 TBD/TODO；每步含完整代码。

**类型一致**：`BrowseQuery{path,into,panel,select}` 定义（Task 1）与 projects_main.html 的 `into=path&panel=browse-panel-add`（Task 2）一致；`BrowseTpl`/`BrowseSelectTpl` 字段与模板变量一致；input id=name 约定（path/dir）在 Task 1 oob 与 Task 2 模板一致。

**已知限制（plan 明确，非 spec 违反）**：browse query 的 path/current/dirs 暂不 percent-encode——路径含空格/中文/`&` 等特殊字符时 query 会断（项目目录通常无空格，YAGNI；实测遇问题再加 `percent-encoding`）。
