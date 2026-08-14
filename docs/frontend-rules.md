# skillkit 前端 AI 约束（server crate 前端部分）

> 本文件约束 skillkit web GUI（`crates/server`）的前端开发方式。类比 project-initialization 按语言给 AI 约束的做法（Java→Spring/前端→本文件）。代码层规范见 CLAUDE.md §2/§7/§8；本文件是它的前端细化。
>
> **定位**：htmx + Askama + rust-embed，无独立前端工程、无 node 构建链、单二进制。**不强制零 JS**——可用轻量原生 JS/htmx 增强交互；**禁止 React/Vue 等重型框架**。

## 1. Non-Negotiables（强规则）

| 必须 | 禁止 |
|------|------|
| 业务逻辑只在 `core` crate，server 是薄壳（handler = load → core → save → 渲染） | 在 Rust handler 里复制 core 的推导/计算逻辑（如 source 名推导） |
| 前端交互优先用 htmx 服务端渲染（`hx-get`/`hx-post`/`hx-delete` + 片段） | 用 React/Vue 等重型框架；引入 node 构建链；加 npm 依赖 |
| 写操作（POST/DELETE）返回**完整页面**（`hx-target="body" hx-swap="outerHTML"`） | 写操作返回片段却用 body outerHTML 替换 |
| SSE 刷新请求 **`?fragment=1` 纯片段**（`hx-target="main" hx-swap="innerHTML"`） | SSE 刷新返回完整页再 select 提取（曾致导航重复） |
| 片段外层固定 id（如 `id="status-panel"`）保证局部替换后 id 不丢 | 片段外层 id 随内容变化（替换后 id 丢失） |
| 新增模板字段/struct 补 `lib.rs` 或 routes 的 re-export | 混用短路径 re-export 与全模块路径（漏 re-export 温床） |
| 注释中文，与文档/commit 一致；Askama 模板同 layout 语言 | 中英混排注释 |
| 动前端源码后跑 `make format && make lint`（rustfmt 管 .rs，模板不动 fmt） | 只改不改验，或跳过 clippy `-D warnings` |

## 2. 前端组织

```
crates/server/
  src/routes/{mod,sources,skills,profiles,projects,sse}.rs   # handler 层（薄壳）
  templates/
    layout.html                     # 唯一 layout：nav + main + SSE 刷新脚本
    {home,sources,skills,profiles,projects,project_workspace}.html  # 页面薄壳
    fragments/
      {home,sources,skills,profiles,projects,workspace}_main.html   # 各视图 main 内容（?fragment=1 用）
      {status,apply_result,profile_skills,source_name_input}.html   # 局部片段（写操作返回）
  static/{htmx.min.js, sortable.min.js, app.css}   # rust-embed 嵌入，不走 CDN
```

**页面 = 薄壳 + include**：每个视图页面模板只写 `{% extends "layout.html" %}{% block content %}{% include "fragments/xxx_main.html" %}{% endblock %}`。main 内容**只存在 fragment 里**，页面模板不得重复内容。

**两套返回**：`page` handler 接受 `Query<FragmentQuery>`，`?fragment=1` 渲染 `*_main` 片段（纯内容），否则渲染完整页（含 layout）。`FragmentQuery` 定义在 `routes/mod.rs`。

## 3. htmx 交互模式

**写操作（install/remove/add/apply 等）**：返回完整页面，`hx-target="body" hx-swap="outerHTML"`。页面模板 include fragment，所以渲染完整页 = 渲染 fragment 进 layout，内容不重复。

**局部片段（status/profile_skills 等）**：写操作返回对应 `fragments/*.html`，`hx-target="#xxx-panel" hx-swap="outerHTML"`，片段外层固定 id。

**SSE 跨进程刷新**：`layout.html` 里 `EventSource` 收到 `changed` → `htmx.ajax('GET', location.pathname + '?fragment=1', { target: 'main', swap: 'innerHTML' })`。**响应必须是纯片段**（不含 nav），从根上杜绝导航重复。

**实时预览**：输入框 `hx-get` + `hx-trigger="input changed delay:300ms"` + `hx-include="this"` + `hx-target="#wrapper" hx-swap="innerHTML"`，服务端推导返回预填 value 的 input 片段（如 `source_name_input.html`）。**推导规则只在 core**，前端不得复制一份。

**拖拽**：SortableJS 已引入（`layout.html` 的 `htmx:afterSettle` 幂等初始化）。列表项重排提交用 `hx-post` + body 手动解析重复 key（`form_urlencoded::parse`），serde_urlencoded 不支持重复 key→Vec。

## 4. Askama 约定（踩坑即规则）

- **模板语法**：`{% extends %}`/`{% block %}`/`{% include %}`。**include 不传变量**（无 `{% include "x" with var %}`）——被 include 模板字段名必须与外层 for 变量名对齐。
- **match 头花括号歧义**：`match Struct{...}.render()` 编译报错。先 `let rendered = Struct{...}.render();` 再 match，或拆同步 `render_xxx` fn。
- **handler 第一参数是 `State<AppState>` extractor**，不能从写操作传裸 state 调 page handler。拆同步 `fn render_xxx(state: AppState, token)`，page 和写操作都调它。
- **方法借用参数**：`contains(&meta.id)` 在 askama 编译失败。handler 预计算 `Vec<(SkillMeta, bool)>`。
- **私有 async fn**：clippy `unused_async` 对无 await 的私有 async fn 报错。`render_xxx` 改同步 fn（pub handler 保持 async）。
- **路径参数编码**：id 含 `/`（`source/skill`）须 `%2F` 编码（handler 预编码），单段 `{id}` 接受解码后 `/`。
- **尾斜杠 404**：axum 0.8 `/{token}` 严格匹配，`/TOKEN/` 需额外注册。
- **scope serde**：`#[serde(rename_all = "lowercase")]`，json 里 `"global"`/`"local"`。
- **htmx 2.x SSE 扩展独立包**：不用 htmx 的 sse 扩展，用浏览器原生 `EventSource` + `htmx.ajax`。

## 5. 测试策略（前端相关）

- **HTTP 层测试**（`crates/server/tests/routes.rs`）：断言状态码 + 响应内容。
- **片段契约测试**：断言 `?fragment=1` 响应**不含 `<nav>` / layout 脚本**、正常页含 nav。这是防导航重复类 bug 的自动防线（curl 级即可验证，不需真实浏览器）。
- **e2e 测试**（`make e2e`）：python playwright 驱动真实 chromium，覆盖 HTTP 层测不到的浏览器内行为（htmx 换页、SSE 时序、导航重复回归）。用例在 `e2e/test_ui.py`，serve 由 Makefile 用固定 `--token e2e-test` + 临时 `$HOME` 拉起（隔离，不碰真实 `~/.skillkit`）。
  - 依赖：pipx python playwright（`$(HOME)/.local/pipx/venvs/playwright/bin/python`，Makefile 变量 `PY` 可覆盖）+ chromium（首次 `playwright install chromium`）；无 pytest，纯脚本 + assert + 退出码。
  - `serve --token <固定值>` 仅用于 e2e/localhost（默认仍是随机 token）。
  - **e2e 不进 `make check`**（慢 + 依赖浏览器 + 需空闲端口），改动 GUI 后跑 `make e2e` 回归。
  - **写操作后等 htmx 换页**：用 `expect(locator).to_have_count()` 轮询，不用 `networkidle`（SSE 长连接会拖死它）；页面打开用 `wait_until="load"`。
- 纯视觉交互（Sortable 拖拽手感）不硬补测试——把「可断言的边界」收敛到响应契约 + e2e 关键路径。

## 6. Red Flags（看到就停）

- 在 handler/template 里复制 core 已有的推导/计算逻辑。
- 新增 React/Vue/npm 依赖，或引入 node 构建步骤。
- SSE 刷新返回完整页面（而非 `?fragment=1` 片段）。
- 写操作返回片段却用 `hx-target="body" hx-swap="outerHTML"`（body 是完整页语义）。
- 片段外层没有固定 id。
- 新前端 struct 类型未补 re-export 就使用。
- 改模板/静态资源后不跑 `make check`（Askama 模板编译错误只有 `make check`/`cargo test` 能暴露）。
- 用 htmx 2.x 的 sse 扩展（拆包了），应走原生 `EventSource`。
- 表单重复 key（checkbox 多选）用 serde 结构体接收（会失败），应手动 `form_urlencoded::parse`。
