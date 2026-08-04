# Projects 路径输入交互升级 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Projects 页两处路径输入升级——「浏览...」目录列表改居中浮层；路径输入框加 Tab 前缀补全。

**Architecture:** htmx 服务端片段不变（browse/complete 都是 GET 片段），前端只换 browse.html 为 overlay 结构 + 新增 complete 端点/片段；关闭浮层与补全键盘走轻量原生 JS（事件委托 + afterSettle 幂等重绑），业务逻辑不碰 core。

**Tech Stack:** Rust + Axum + Askama（服务端渲染片段）+ htmx + 原生 JS + CSS（rust-embed 嵌入静态资源）。

**Spec:** `docs/superpowers/specs/2026-08-03-projects-browse-floating-panel-design.md`

## Global Constraints

- 路径不硬编码：模板用 `{{ token }}`，handler 用 `resolve_dir` / `dirs::home_dir()`。
- 业务逻辑只在 core：browse / complete handler 是薄壳（路径解析 + 列目录是 server 层数据获取），浮层关闭 / 补全键盘纯前端 UI。
- htmx 片段优先：数据交互走 htmx；仅关闭 / 键盘高亮用原生 JS。
- 片段外层 id 固定：`#browse-panel-*` / `#complete-*` id 不随内容变。
- 改模板 / 静态资源后必跑 `make check`（Askama 模板错只有 check 能暴露）。
- commit message 中文 + Conventional Commits。
- **Git：按主人全局习惯，未获明确指示前不自动 add/commit/push。计划中 commit step 是 TDD 节奏建议，执行时由主人或执行模式决定是否真提交。**
- Askama 坑（frontend-rules §4）：`{% for c in &candidates %}` 取字段用 `c.short`；结构体字段必须 `pub`。

---

### Task 1: complete 端点 + 候选片段（Rust TDD）

新增 `GET /{token}/projects/complete?path=<P>&panel=<id>`：拆 base/prefix + 复用 `list_subdirs` + 前缀过滤，渲染候选片段。

**Files:**
- Create: `crates/server/templates/fragments/complete.html`
- Modify: `crates/server/src/routes/projects.rs`（在 `browse` handler 后追加）
- Modify: `crates/server/src/routes/mod.rs:60`（browse 路由后追加 complete 路由，必须在 `projects/{id}` 参数路由前）
- Test: `crates/server/tests/routes.rs`（追加 3 个测试）

**Interfaces:**
- Consumes: `resolve_dir(Option<&str>) -> PathBuf`、`list_subdirs(&StdPath) -> std::io::Result<Vec<String>>`（projects.rs 现有私有 fn，同模块直接调）
- Produces: `pub async fn complete(Path<String>, Query<CompleteQuery>) -> Response`；路由 `GET /{token}/projects/complete`

- [ ] **Step 1: 写失败测试（追加到 `crates/server/tests/routes.rs` 末尾）**

```rust
#[tokio::test]
async fn projects_complete_lists_prefix_matched_subdirs() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().display().to_string();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    std::fs::create_dir_all(dir.path().join("lab")).unwrap();
    std::fs::create_dir_all(dir.path().join("labx")).unwrap();
    std::fs::create_dir_all(dir.path().join("other")).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/test-token/projects/complete?path={base}/la&panel=complete-path"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("lab/"), "前缀 la 匹配含 lab");
    assert!(body.contains("labx/"), "前缀 la 匹配含 labx");
    assert!(!body.contains("other"), "不含非前缀目录");
    assert!(
        body.contains(&format!(r#"data-path="{base}/lab/""#)),
        "data-path 是完整路径带尾斜杠"
    );
}

#[tokio::test]
async fn projects_complete_trailing_slash_lists_all_subdirs() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().display().to_string();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    std::fs::create_dir_all(dir.path().join("b")).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/test-token/projects/complete?path={base}/&panel=complete-path"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = common::body_string(resp).await;
    assert!(body.contains("a/"), "尾斜杠=prefix 空，列全部子目录");
    assert!(body.contains("b/"));
}

#[tokio::test]
async fn projects_complete_no_match_returns_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().display().to_string();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    std::fs::create_dir_all(dir.path().join("lab")).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/test-token/projects/complete?path={base}/zzz&panel=complete-path"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = common::body_string(resp).await;
    assert!(body.contains(r#"class="complete-list""#), "空也返回容器");
    assert!(!body.contains("complete-item"), "无匹配项不含候选");
}
```

- [ ] **Step 2: 跑测试验证失败**

Run: `cargo test -p skillkit-server projects_complete -- --nocapture`
Expected: 3 个测试 FAIL（路由未注册，返回 404，断言 status OK / body 不符）。

- [ ] **Step 3: 创建候选片段模板 `crates/server/templates/fragments/complete.html`**

```html
<div id="{{ panel }}" class="complete-list">
  {% for c in &candidates %}
  <div class="complete-item" data-path="{{ c.full }}/">{{ c.short }}/</div>
  {% endfor %}
</div>
```

- [ ] **Step 4: 在 `crates/server/src/routes/projects.rs` 追加 complete 相关代码**

插入位置：`browse` handler 之后、`resolve_dir` fn 之前（与 browse 同区）。

```rust
#[derive(Deserialize)]
pub struct CompleteQuery {
    pub path: String,
    pub panel: String,
}

/// 候选项：short=子目录名（显示），full=base/子目录（data-path 回填）。
pub struct Candidate {
    pub short: String,
    pub full: String,
}

#[derive(Template)]
#[template(path = "fragments/complete.html")]
pub struct CompleteTpl<'a> {
    pub token: &'a str,
    pub panel: &'a str,
    pub candidates: Vec<Candidate>,
}

/// 路径输入框 Tab 补全：拆「基准目录 + 前缀」。
/// - 尾斜杠或空 → base=path（解析后），prefix=""（列全部子目录）
/// - 否则 → base=parent，prefix=末段（前缀匹配）
/// ~ / 空按 home 解析（复用 resolve_dir）。
fn split_prefix(raw: &str) -> (PathBuf, String) {
    let raw = raw.trim();
    if raw.is_empty() || raw.ends_with('/') {
        return (resolve_dir(Some(raw)), String::new());
    }
    let resolved = resolve_dir(Some(raw));
    let prefix = resolved
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = resolved.parent().map(PathBuf::from).unwrap_or(resolved);
    (base, prefix)
}

/// Tab 补全：列 base 下前缀匹配的子目录候选，渲染 complete.html。
pub async fn complete(
    Path(token): Path<String>,
    Query(q): Query<CompleteQuery>,
) -> Response {
    let (base, prefix) = split_prefix(&q.path);
    let candidates: Vec<Candidate> = list_subdirs(&base)
        .unwrap_or_default()
        .into_iter()
        .filter(|name| name.starts_with(&prefix))
        .map(|name| {
            let full = base.join(&name).to_string_lossy().into_owned();
            Candidate { short: name, full }
        })
        .collect();
    let rendered = CompleteTpl {
        token: &token,
        panel: &q.panel,
        candidates,
    }
    .render();
    render_str(rendered)
}
```

- [ ] **Step 5: 注册路由 `crates/server/src/routes/mod.rs`**

在 browse 路由行（`mod.rs:60`）之后插入一行：

```rust
        .route("/{token}/projects/complete", get(projects::complete))
```

完整上下文（mod.rs:58-61 改后）：
```rust
        .route("/{token}/projects", get(projects::list).post(projects::add))
        .route("/{token}/projects/scan", post(projects::scan))
        .route("/{token}/projects/browse", get(projects::browse))
        .route("/{token}/projects/complete", get(projects::complete))
```

注意：必须在 `/{token}/projects/{id}` 参数路由之前，否则 `complete` 被当成 `{id}` 匹配。

- [ ] **Step 6: 跑测试验证通过**

Run: `cargo test -p skillkit-server projects_complete -- --nocapture`
Expected: 3 个测试 PASS。

- [ ] **Step 7: 全量 check**

Run: `make check`
Expected: 全绿（core 51 + cli 单元 17 + cli e2e 9 + server 39[+3] + clippy 零 warning）。

- [ ] **Step 8: Commit**

```bash
git add crates/server/templates/fragments/complete.html \
        crates/server/src/routes/projects.rs \
        crates/server/src/routes/mod.rs \
        crates/server/tests/routes.rs
git commit -m "feat(server): projects 路径输入框 Tab 补全端点（前缀匹配候选）"
```

---

### Task 2: browse.html 浮层化

把目录浏览片段从平铺 `<div class="browse-panel">` 改为 `.browse-overlay`（遮罩）+ `.browse-modal`（居中模态）；顶层去 id 修嵌套 bug；加 ✕ 关闭按钮。

**Files:**
- Modify: `crates/server/templates/fragments/browse.html`（整体重写）
- Test: `crates/server/tests/routes.rs`（3 个 browse 测试追加 overlay 断言）

**Interfaces:**
- Consumes: handler `browse` 不变（渲染 browse.html）；挂载点 `#browse-panel-add` / `#browse-panel-scan`（projects_main.html，Task 3 保留）
- Produces: browse.html 渲染 `.browse-overlay > .browse-modal`，「进入/上级」`hx-target=#{{panel}}` 指挂载点

- [ ] **Step 1: 重写 `crates/server/templates/fragments/browse.html`**

```html
<div class="browse-overlay">
  <div class="browse-modal" role="dialog" aria-label="选择目录">
    <header class="browse-header">
      <span class="browse-cwd" title="{{ current }}">📁 {{ current }}</span>
      <button type="button" class="browse-close" aria-label="关闭">✕</button>
    </header>
    {% if parent != current %}
    <div class="browse-toolbar">
      <button type="button"
              hx-get="/{{ token }}/projects/browse?path={{ parent }}&into={{ into }}&panel={{ panel }}"
              hx-target="#{{ panel }}">↑ 上级</button>
    </div>
    {% endif %}
    <div class="browse-body">
      {% if dirs.is_empty() %}
      <p class="muted browse-empty">（无子目录）</p>
      {% else %}
      <ul class="browse-list">
        {% for d in dirs %}
        <li>
          <span class="browse-name">📁 {{ d }}/</span>
          <span class="browse-ops">
            <button type="button"
                    hx-get="/{{ token }}/projects/browse?path={{ current }}/{{ d }}&into={{ into }}&panel={{ panel }}"
                    hx-target="#{{ panel }}">进入</button>
            <button type="button"
                    hx-get="/{{ token }}/projects/browse?path={{ current }}&select={{ d }}&into={{ into }}&panel={{ panel }}"
                    hx-swap="none">✓ 选定</button>
          </span>
        </li>
        {% endfor %}
      </ul>
      {% endif %}
    </div>
  </div>
</div>
```

- [ ] **Step 2: 更新 `projects_browse_lists_subdirs_skips_hidden_and_files` 测试（routes.rs:907）**

在现有断言后追加（`assert!(body.contains("上级"))` 之后）：

```rust
    assert!(body.contains(r#"class="browse-overlay""#), "浮层遮罩");
    assert!(body.contains(r#"class="browse-modal""#), "模态卡片");
    assert!(body.contains(r#"class="browse-close""#), "关闭按钮");
```

- [ ] **Step 3: 全量 check**

Run: `make check`
Expected: 全绿（3 个 browse 测试新断言通过；「进入/选定/上级/子目录」文本断言仍成立——重写后保留）。

- [ ] **Step 4: Commit**

```bash
git add crates/server/templates/fragments/browse.html crates/server/tests/routes.rs
git commit -m "feat(gui): 目录浏览改居中浮层（遮罩+模态，修顶层 id 嵌套）"
```

---

### Task 3: projects_main.html 输入框补全挂载点

`#path` / `#dir` 各包 `.input-wrap`（相对定位），输入框加 `data-complete`，下方加 `.complete-panel` 候选挂载点。浏览按钮与 `browse-panel-*` 不动。

**Files:**
- Modify: `crates/server/templates/fragments/projects_main.html`（注册 / 扫描两处 input 改造）
- Test: `crates/server/tests/routes.rs`（`projects_main_renders_browse_buttons_and_panels` 追加断言）

**Interfaces:**
- Consumes: Task 1 的 complete 端点（前端 JS 调）
- Produces: `input[data-complete="complete-path"|"complete-dir"]` + `#complete-path` / `#complete-dir` 候选挂载点

- [ ] **Step 1: 重写 `crates/server/templates/fragments/projects_main.html`**

```html
<h1>Projects</h1>

<section class="card">
  <h2>注册项目</h2>
  <p class="hint">已知项目路径？直接填路径注册。</p>
  <form class="inline" hx-post="/{{ token }}/projects"
        hx-target="body" hx-swap="outerHTML">
    <div class="input-wrap">
      <input id="path" name="path" type="text" placeholder="项目绝对路径（如 /Users/me/app）" required data-complete="complete-path">
      <div class="complete-panel" id="complete-path"></div>
    </div>
    <button type="button"
            hx-get="/{{ token }}/projects/browse?into=path&panel=browse-panel-add"
            hx-target="#browse-panel-add"
            hx-include="#path">浏览...</button>
    <input name="agents" placeholder="agents（可选，逗号分隔）">
    <button>注册</button>
  </form>
  <div id="browse-panel-add"></div>
</section>

<section class="card">
  <h2>扫描发现</h2>
  <p class="hint">不确定有哪些项目？扫目录树自动发现候选。</p>
  <form class="inline" hx-post="/{{ token }}/projects/scan"
        hx-target="#scan-results" hx-swap="outerHTML"
        hx-indicator="#scan-indicator">
    <div class="input-wrap">
      <input id="dir" name="dir" type="text" placeholder="扫描根目录（如 ~/code）" required data-complete="complete-dir">
      <div class="complete-panel" id="complete-dir"></div>
    </div>
    <button type="button"
            hx-get="/{{ token }}/projects/browse?into=dir&panel=browse-panel-scan"
            hx-target="#browse-panel-scan"
            hx-include="#dir">浏览...</button>
    <input type="number" name="depth" value="3" min="0" max="5">
    <button>扫描</button>
    <span id="scan-indicator" class="htmx-indicator">扫描中…</span>
  </form>
  <div id="browse-panel-scan"></div>
  <div id="scan-results"></div>
</section>

<h2>已注册项目</h2>
<ul class="project-list">
  {% for row in &rows %}
  <li>
    <a href="/{{ token }}/projects/{{ row.id }}">{{ row.name }}</a>
    <span class="muted">{{ row.path }}</span>
    <span class="badge">{{ row.local_count }} local skills</span>
    <button class="x"
            hx-delete="/{{ token }}/projects/{{ row.id }}"
            hx-target="body" hx-swap="outerHTML"
            hx-confirm="注销后该项目不再被 skillkit 管理，已落地文件保留。确定？">删除</button>
  </li>
  {% else %}
  <li class="muted">— 还没有注册的项目</li>
  {% endfor %}
</ul>
```

- [ ] **Step 2: 更新 `projects_main_renders_browse_buttons_and_panels` 测试（routes.rs:1007）**

在现有断言末尾（`assert!(body.contains(r#"id="browse-panel-scan""#))` 之后）追加：

```rust
    assert!(body.contains(r#"class="input-wrap""#), "输入框包裹层");
    assert!(body.contains(r#"data-complete="complete-path""#), "注册输入框 data-complete");
    assert!(body.contains(r#"id="complete-path""#), "注册候选挂载点");
    assert!(body.contains(r#"data-complete="complete-dir""#), "扫描输入框 data-complete");
    assert!(body.contains(r#"id="complete-dir""#), "扫描候选挂载点");
```

- [ ] **Step 3: 全量 check**

Run: `make check`
Expected: 全绿（main 测试新断言通过；旧 input id / browse 按钮 / panel 断言仍成立——改造后保留）。

- [ ] **Step 4: Commit**

```bash
git add crates/server/templates/fragments/projects_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): 路径输入框加 Tab 补全挂载点（input-wrap + complete-panel）"
```

---

### Task 4: app.css 浮层 + 补全样式

新增 `.browse-*`（遮罩 + 模态 + 滚动列表）和 `.input-wrap / .complete-*`（补全 dropdown）样式。

**Files:**
- Modify: `crates/server/static/app.css`（末尾追加）

- [ ] **Step 1: 在 `crates/server/static/app.css` 末尾追加**

```css
/* ---------- 目录浏览浮层 ---------- */
.browse-overlay {
  position: fixed; inset: 0; z-index: 100;
  background: rgba(28, 25, 23, .42);
  display: flex; align-items: center; justify-content: center;
  padding: 24px;
}
.browse-modal {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 10px;
  box-shadow: var(--shadow);
  width: 560px; max-width: 100%;
  max-height: min(70vh, 560px);
  display: flex; flex-direction: column;
  overflow: hidden;
}
.browse-header {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--line);
  background: var(--surface-2);
}
.browse-cwd {
  font-family: var(--mono); font-size: 12px; color: var(--ink-2);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.browse-close {
  padding: 2px 8px; color: var(--ink-3); border: none; background: none;
  font-size: 14px; line-height: 1; cursor: pointer;
}
.browse-close:hover { color: var(--danger); }
.browse-toolbar { padding: 8px 14px; border-bottom: 1px solid var(--line-2); }
.browse-body { overflow: auto; flex: 1; padding: 4px 0; }
.browse-empty { padding: 16px 14px; }
.browse-list { list-style: none; }
.browse-list li {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 8px 14px; border-bottom: 1px solid var(--line-2);
}
.browse-list li:last-child { border-bottom: none; }
.browse-list li:hover { background: var(--surface-2); }
.browse-name {
  font-family: var(--mono); font-size: 12.5px; color: var(--ink);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.browse-ops { display: inline-flex; gap: 4px; flex-shrink: 0; }

/* ---------- 路径输入框 Tab 补全 ---------- */
.input-wrap { position: relative; display: inline-flex; align-items: center; }
.complete-list {
  position: absolute; top: calc(100% + 2px); left: 0;
  min-width: 100%; max-width: 440px;
  max-height: 240px; overflow: auto;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  box-shadow: var(--shadow);
  z-index: 90;
}
.complete-item {
  padding: 6px 12px;
  font-family: var(--mono); font-size: 12.5px; color: var(--ink);
  cursor: pointer;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.complete-item:hover, .complete-item.active { background: var(--accent-soft); color: var(--accent); }
```

- [ ] **Step 2: check（确保静态资源嵌入不破）**

Run: `make check`
Expected: 全绿（CSS 不参与编译，但 rust-embed 嵌入 + 全量测试无回归）。

- [ ] **Step 3: Commit**

```bash
git add crates/server/static/app.css
git commit -m "feat(gui): 目录浏览浮层 + 路径补全 dropdown 样式"
```

---

### Task 5: layout.html 浮层关闭 + Tab 补全 JS

`<script>` 追加：浮层关闭（✕ / 遮罩 / ESC）+ Tab 补全键盘（Tab 拉 / ↓↑ 高亮 / Enter 补全 / Esc 关）+ afterSettle 幂等重绑。

**Files:**
- Modify: `crates/server/templates/layout.html`（`<script>` 内，`var es = new EventSource(...)` 之后、`</script>` 之前追加）

**Interfaces:**
- Consumes: Task 1 complete 端点；Task 3 的 `input[data-complete]` + `.complete-panel`
- Produces: 浮层关闭交互 + 补全键盘交互

- [ ] **Step 1: 在 `crates/server/templates/layout.html` 的 `<script>` 末尾追加**

定位：现有 `var es = new EventSource('/{{ token }}/events'); ...` 块之后、`</script>` 之前。

```js
    // ===== 目录浏览浮层关闭：✕ / 点遮罩 / ESC（纯 UI 收起，不涉业务） =====
    function closeBrowseOverlay(overlay) { overlay.remove(); }
    document.body.addEventListener('click', function (e) {
      var overlay = e.target.closest('.browse-overlay');
      if (!overlay) return;
      if (e.target.closest('.browse-close') || e.target === overlay) {
        e.preventDefault();
        closeBrowseOverlay(overlay);
      }
    });
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') {
        document.querySelectorAll('.browse-overlay').forEach(closeBrowseOverlay);
      }
    });

    // ===== 路径输入框 Tab 补全 =====
    var SK_TOKEN = '{{ token }}';
    function moveCompleteActive(items, dir) {
      if (!items.length) return;
      var arr = Array.prototype.slice.call(items);
      var idx = arr.findIndex(function (i) { return i.classList.contains('active'); });
      if (idx < 0) idx = 0;
      arr[idx].classList.remove('active');
      idx = (idx + dir + arr.length) % arr.length;
      arr[idx].classList.add('active');
      arr[idx].scrollIntoView({ block: 'nearest' });
    }
    function setupComplete(input, panelId) {
      var panel = document.getElementById(panelId);
      if (!panel) return;
      input.addEventListener('keydown', function (e) {
        var items = panel.querySelectorAll('.complete-item');
        if (e.key === 'Tab') {
          e.preventDefault();
          var val = encodeURIComponent(input.value);
          htmx.ajax('GET', '/' + SK_TOKEN + '/projects/complete?path=' + val + '&panel=' + panelId,
                    { target: panel, swap: 'innerHTML' });
        } else if (e.key === 'ArrowDown') {
          e.preventDefault();
          moveCompleteActive(items, 1);
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          moveCompleteActive(items, -1);
        } else if (e.key === 'Enter') {
          var active = panel.querySelector('.complete-item.active');
          if (active) { e.preventDefault(); input.value = active.dataset.path; panel.innerHTML = ''; }
        } else if (e.key === 'Escape') {
          panel.innerHTML = '';
        }
      });
      panel.addEventListener('htmx:afterSettle', function () {
        var first = panel.querySelector('.complete-item');
        if (first) first.classList.add('active');
      });
      input.addEventListener('blur', function () {
        setTimeout(function () { panel.innerHTML = ''; }, 150);
      });
      panel.addEventListener('mousedown', function (e) {
        var item = e.target.closest('.complete-item');
        if (item) { input.value = item.dataset.path; panel.innerHTML = ''; }
      });
    }
    // SSE 刷新 main 后幂等重绑（data-complete-bound 防重复，仿 Sortable）
    document.body.addEventListener('htmx:afterSettle', function () {
      document.querySelectorAll('input[data-complete]:not([data-complete-bound])').forEach(function (input) {
        input.setAttribute('data-complete-bound', '1');
        setupComplete(input, input.dataset.complete);
      });
    });
```

- [ ] **Step 2: check**

Run: `make check`
Expected: 全绿（layout.html 是 Askama 模板，`{{ token }}` 编译通过）。

- [ ] **Step 3: Commit**

```bash
git add crates/server/templates/layout.html
git commit -m "feat(gui): 浮层关闭 + 路径 Tab 补全键盘交互（事件委托+幂等重绑）"
```

---

### Task 6: 全量验证 + 手动走查

确保所有改动协同工作，手动走查两条交互链路。

**Files:** 无改动，纯验证。

- [ ] **Step 1: 全量 check**

Run: `make check`
Expected: 全绿（core 51 + cli 单元 17 + cli e2e 9 + server 42[+3 complete +新断言] + clippy 零 warning）。

- [ ] **Step 2: 起服务手动走查**

Run: `make run ARGS="serve --port 7317"`，浏览器开 `http://localhost:7317/<token>/projects`（token 见启动输出）。

浏览浮层走查：
- [ ] 注册表单点「浏览...」→ 浮层居中弹出 + 半透明遮罩盖住页面。
- [ ] 目录很多时 `.browse-body` 出现滚动条（固定大小，内容超出滚动）。
- [ ] 点「进入」子目录 → 浮层内列表更新，浮层不闪不重开。
- [ ] 点「↑上级」→ 回上级目录列表。
- [ ] 点「✓选定」→ 输入框回填路径 + 浮层消失。
- [ ] 点 ✕ / 点遮罩空白 / 按 ESC → 浮层关闭。
- [ ] 扫描表单「浏览...」同理（独立浮层实例）。

Tab 补全走查：
- [ ] `#path` 输入 `/Users/mywo/la`（打到一半）按 Tab → 输入框下方列 `la` 开头子目录候选，首项高亮。
- [ ] ↓/↑ 循环移动高亮，候选超出时自动滚动到可见。
- [ ] 回车 → 输入框补全为高亮候选完整路径（带尾斜杠，如 `/Users/mywo/lab/`）+ 候选关闭。
- [ ] 补全后再按 Tab → 列新路径子目录（逐级续补）。
- [ ] ESC / 输入框失焦 → 候选关闭。
- [ ] 候选为空时回车 → 正常提交表单（不被补全拦截）。
- [ ] 鼠标点候选项 → 补全到输入框。
- [ ] `#dir`（扫描根目录）同理。

- [ ] **Step 3: （可选）GUI e2e**

Run: `make e2e`
Expected: 现有 6 用例不回归（不覆盖 projects 视图）。增量覆盖由主人定是否加（见 spec §7）。

- [ ] **Step 4: 报告**

确认：make check 全绿 + 两条交互链路走查通过。如发现 bug，回对应 Task 修复后重跑。

---

## 实现期增量（已实现，commit cc06909）

### Task 7: scan 浮层 + toggle

**Files:** `templates/fragments/scan_results.html`（浮层化）、`templates/fragments/scan_toggle.html`（**新增** toggle 按钮 fragment）、`src/routes/projects.rs`（`ScanCandidate{path,registered}` + scan handler load projects 标记 + `toggle` handler + `ToggleTpl/Form`）、`src/routes/mod.rs`（toggle 路由）、`templates/layout.html`（scan 浮层开时跳过 SSE + 关闭刷新 main）、`static/app.css`（`.scan-toggle-btn.registered`）。

- scan_results.html 浮层化（复用 `.browse-overlay`，加 `.scan-flyout`）；候选 `ScanCandidate{path, registered}`。
- scan handler load 所有 projects 建 canonical path set，候选 canonicalize 后比对标记 registered。
- toggle 端点 `POST /{token}/projects/toggle`：canonical 精确匹配，已注册→`Project::remove`、未注册→`Project::register`+save，返回 scan_toggle.html 按钮 fragment（`hx-swap=outerHTML` 替换 form，浮层保持）。
- layout SSE：scan 浮层开时跳过整页刷新；`closeBrowseOverlay` 对 scan-flyout 关闭时刷新 main。
- 验证：curl（scan 30 候选 + 5 已注册精确匹配 + toggle 注册/注销循环）+ playwright（两次扫描 + toggle + 关闭）。

### Task 8: hx-swap 继承 bug 修复

**Files:** `templates/fragments/projects_main.html`（三个触发按钮显式 `hx-swap="innerHTML"`）。

- 现象：浏览浮层内「进入」无反应；选定后关闭再点「浏览」不弹。
- 根因：触发按钮在 `<form hx-swap="outerHTML">` 内、自己没写 hx-swap → 继承 form 的 outerHTML → 整个挂载点被浮层替换（顶层无 id）→ 挂载点 id 丢失 → 后续 `hx-target="#挂载点"` 找不到。
- 修复：注册浏览 / 扫描浏览按钮加 `hx-swap="innerHTML"`；扫描 form `hx-swap="outerHTML"` → `"innerHTML"`。
- 验证：playwright Bug1（浏览→进入 cwd 变 + 挂载点保留）+ Bug2（选定→再浏览浮层重弹）+ scan 两次扫描；make check 全绿。

## Self-Review（增量 vs 实现）

- Task 7 scan 浮层 toggle 全覆盖（模板 + handler + 路由 + SSE + 样式 + curl/playwright 验证）。✓
- Task 8 bug 修复根因（hx-swap 继承）+ 修复（显式 innerHTML）+ 验证（playwright 复现通过）。✓
- commit cc06909 含 Task 1-8 全部改动。

### Task 9: review 后 5 项改进（commit 7b6f94c）

**Files:** `src/routes/projects.rs`（`BrowseQuery.path` alias / `ScanForm` 去 depth / `ProjectAddForm` 去 agents / add 查重 + resolve_dir / scan resolve_dir / Tpl message / render_list message）、`templates/fragments/projects_main.html`（去 agents/浏览/depth + message 顶部提示）、`tests/routes.rs`（断言更新 + browse dir alias + add 查重）。

- browse `path` 加 `#[serde(alias="dir")]`（扫描浏览 name=dir 正确传入）。
- scan/browse 用 `resolve_dir` 展开 `~`（修 `~/...` 不识别）。
- 注册去 agents/浏览；扫描去 depth（固定默认 3）。
- add 查重（canonical 精确匹配）+ `ProjectsTpl.message` 顶部提示。
- 验证：curl（browse?dir=~ cwd 正确 / scan ~ 35 候选 / add 重复拒绝）+ 测试（projects_main_renders 反向断言 / browse dir alias / add 查重）+ make check 全绿（server 41）。

---

## Self-Review（plan vs spec）

**1. Spec 覆盖：**
- 浏览浮层化（spec §3）→ Task 2（模板）+ Task 4（CSS）+ Task 5（关闭 JS）。✓
- Tab 补全（spec §4）→ Task 1（端点）+ Task 3（挂载点）+ Task 4（CSS）+ Task 5（键盘 JS）。✓
- 顶层去 id + hx-target 指挂载点（spec §3.1）→ Task 2 browse.html。✓
- 关闭四路（spec §3.2）→ Task 5 JS（✕/遮罩/ESC + 选定复用 browse_select 不动）。✓
- 前缀匹配 + 带尾斜杠（spec §4.1）→ Task 1 split_prefix + complete.html `data-path="{{ c.full }}/"`。✓
- 边界与不变量（spec §6）→ Global Constraints。✓
- 测试要点（spec §7）→ Task 1 单测 + Task 6 手动走查。✓

**2. 占位符扫描：** 无 TBD / TODO / "适当处理" / "类似 Task N"。每个 step 含完整代码或精确命令。✓

**3. 类型一致性：**
- `complete(Path<String>, Query<CompleteQuery>)` — Task 1 定义，Task 5 JS 调 `/{token}/projects/complete`。✓
- `CompleteQuery { path, panel }` — Task 1 定义，测试 + JS 都用 path/panel。✓
- `Candidate { short, full }` — Task 1 定义，complete.html 用 `c.short` / `c.full`。✓
- `split_prefix(raw) -> (PathBuf, String)` — Task 1 定义，`complete` 调 `(base, prefix) = split_prefix(...)`。✓
- `data-complete="complete-path"|"complete-dir"` — Task 3 模板写，Task 5 JS `input.dataset.complete` 读。✓
- `#complete-path` / `#complete-dir` — Task 3 模板 id，Task 5 JS `getElementById(panelId)`。✓
- `.browse-overlay/.browse-modal/.browse-close` — Task 2 模板 class，Task 4 CSS，Task 5 JS `closest('.browse-overlay')` / `closeBrowseOverlay`。✓
- `.complete-item.active` / `dataset.path` — Task 1 complete.html `data-path`，Task 5 JS 读。✓

无类型 / 命名不一致。计划可执行。
