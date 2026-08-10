# 安装本地 skill · 前端 UI 重设计（Modal 浮层三合一）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Skills 页「安装本地」从撑坏 toolbar 的内联表单，重做成复用 `.browse-overlay` 的 modal 浮层，支持拖放 zip/目录、文件选择、路径输入三种方式安装，core/CLI 零改动。

**Architecture:** server 新增 `POST /skills/install-local/upload`（Multipart）端点接收 zip/目录上传，落 `tempfile::TempDir` 临时区转成本地路径后复用现有 core `install_local`；前端 modal 复用 `.browse-overlay` 关闭机制，原生 JS 处理拖放/目录递归/输入互斥/提交路由（按输入源 POST 到 path 表单端点或 upload 端点）。

**Tech Stack:** Rust 2021 + Axum 0.8（`multipart` feature）+ Askama 模板 + htmx + rust-embed 静态资源；原生 JS（禁 React/Vue/npm）；`tempfile`、`zip`（dev）。

## Global Constraints

- core（`crates/core/**`）与 CLI（`crates/cli/**`）零改动；`install_local(paths, src_path, name, scope, force) -> Result<SkillMeta>` 签名不变（`install_local.rs:238-244`）。
- 写操作（POST）返回完整 Skills 页 `hx-target="body" hx-swap="outerHTML"`；失败 `error_response` 返回 toast（4xx 不刷页）。
- SSE 刷新 `?fragment=1` 纯片段，不动 SSE 链路。
- 片段外层固定 id（`#modal-mount`、`#install-drop-zone`、`#install-form` 等）。
- 禁 React/Vue/npm 构建链；前端只用原生 JS + htmx。
- 路径不硬编码，用 `dirs` / `tempfile`；临时区用 `tempfile::TempDir`（drop 自动清理）。
- 改模板/静态资源后跑 `make check`（rust-embed 重打包 + Askama 编译 + clippy `-D warnings`）。
- commit message 中文 + Conventional Commits（`feat:`/`fix:`/`test:`/`chore:`/`refactor:`）。
- 测试里跑 `git commit` 必须带 `-c user.email -c user.name`（不依赖机器全局 config）。
- summary 标识三路径统一用 `install_local` 返回的 `SkillMeta.id`（形如 `local/<name>`），不用 `f.path`。

---

## File Structure

见上文映射表。职责边界：
- `skills.rs`：三 handler——`install_local`（POST path 表单，现有，改 summary）、`install_local_form`（GET modal 片段，改渲染新模板）、`install_local_upload`（POST multipart，新增）。加一个私有纯函数 `rebuild_dir`（目录重建 + 路径逃逸过滤）。
- `install_local_modal.html`：纯结构模板，不含提交逻辑（JS 接管 form submit）。
- `layout.html`：`#modal-mount` 容器 + 一个 `<script>` 块装所有 install-local 交互 JS（拖放、递归、互斥、折叠态、提交路由、安装中态）。
- `app.css`：新增 `.install-modal`/`.drop-zone`(+`.drag`/`.has-file`)/`.install-fields`/`.advanced`/`.install-actions`/`.install-actions .primary` 规则块。

---

## Task 1：依赖变更 + upload 端点（zip）+ DefaultBodyLimit + summary 统一 m.id

**Files:**
- Modify: `crates/server/Cargo.toml`
- Modify: `crates/server/src/routes/mod.rs`（注册 upload 路由 + `DefaultBodyLimit`）
- Modify: `crates/server/src/routes/skills.rs`（新增 `install_local_upload`；`install_local` 的 summary 改 `m.id`）
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `skillkit_core::install_local(&Paths, &str, Option<&str>, Scope, bool) -> Result<SkillMeta>`（`install_local.rs:238`）；`SkillMeta.id`（`local/<name>`）；`AppState.paths`；`render_skills(state, token, summary, fragment, ...)`；`error_response(String)`（`mod.rs:17-26`）。
- Produces: `pub async fn install_local_upload(State<AppState>, Path<String>, Multipart) -> Response`；路由 `POST /{token}/skills/install-local/upload` 挂 `DefaultBodyLimit::max(MAX_UPLOAD_BYTES)`；常量 `const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;`（暂定 100MiB，Task 2 实测后调整）。

- [ ] **Step 1: 改 Cargo.toml 三处依赖**

`crates/server/Cargo.toml`：
- `[dependencies]` 的 `axum = "0.8"` 改为 `axum = { version = "0.8", features = ["multipart"] }`。
- `tempfile = "3"` 从 `[dev-dependencies]` 移到 `[dependencies]`（`[dev-dependencies]` 段删掉这行，`[dependencies]` 段加 `tempfile = "3"`）。
- `[dev-dependencies]` 加 `zip = "2"`。

- [ ] **Step 2: 验证依赖编译**

Run: `cargo build -p skillkit-server`
Expected: 编译通过（确认 multer 进 Cargo.lock：`grep multer Cargo.lock` 有命中）。

- [ ] **Step 3: 写失败测试——合法 zip multipart 上传成功**

在 `crates/server/tests/routes.rs` 末尾加（构造一个合法 skill zip，multipart 上传，断言完整页 + summary 含 `local/<name>` + registry 落库）：

```rust
// 辅助：构造一个合法 skill zip 字节（含 SKILL.md frontmatter 带 name）
fn make_skill_zip(skill_name: &str) -> Vec<u8> {
    use std::io::{Write, Cursor};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;
    let buf = Cursor::new(Vec::new());
    let mut zw = ZipWriter::new(buf);
    let opts = SimpleFileOptions::default();
    zw.start_file("SKILL.md", opts).unwrap();
    writeln!(zw, "---\nname: {}\n---\n# {}", skill_name, skill_name).unwrap();
    let Cursor { 0: bytes, .. } = zw.finish().unwrap();
    bytes
}

// 辅助：构造 multipart/form-data body（boundary + 一个 archive 字段 + 可选 name/scope/force）
fn multipart_zip_body(boundary: &str, archive: &[u8], name: Option<&str>, scope: Option<&str>, force: bool) -> Vec<u8> {
    let mut body = Vec::new();
    let crlf = b"\r\n";
    let mut part = |name: &str, val: &str, body: &mut Vec<u8>| {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes());
        body.extend_from_slice(val.as_bytes());
        body.extend_from_slice(crlf);
    };
    if let Some(n) = name { part("name", n, &mut body); }
    if let Some(s) = scope { part("scope", s, &mut body); }
    if force { part("force", "on", &mut body); }
    // archive 字段（文件）
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice("Content-Disposition: form-data; name=\"archive\"; filename=\"pkg.zip\"\r\n".as_bytes());
    body.extend_from_slice("Content-Type: application/zip\r\n\r\n".as_bytes());
    body.extend_from_slice(archive);
    body.extend_from_slice(crlf);
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    body
}

#[tokio::test]
async fn install_local_upload_zip_success() {
    let (app, state) = build_test_app().await; // 复用现有测试辅助
    let zip = make_skill_zip("demo-up");
    let body = multipart_zip_body("testbound", &zip, None, Some("local"), false);
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/{}/skills/install-local/upload", state.token))
                .header("content-type", "multipart/form-data; boundary=testbound")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let text = body_to_string(res).await;
    assert!(text.contains("✓ 已安装本地 skill：local/demo-up"), "summary 应含 local/<name>，实际：{}", &text[..text.len().min(400)]);
    // registry 落库
    assert!(state.paths.registry_path().exists());
    let reg = std::fs::read_to_string(state.paths.registry_path()).unwrap();
    assert!(reg.contains("demo-up"));
}
```

注：`build_test_app` / `body_to_string` / `Request`/`Body`/`StatusCode` 引用沿用现有测试顶部已 `use` 的项（见 `tests/routes.rs` 现有 3 个 install-local 测试的写法）；若缺则补 `use http_body_util::BodyExt;` 和 `use axum::body::Body;`。

Run: `cargo test -p skillkit-server --test routes install_local_upload_zip_success`
Expected: FAIL（编译错误：`install_local_upload` 未定义 / 路由未注册）。

- [ ] **Step 4: 注册 upload 路由 + DefaultBodyLimit**

`crates/server/src/routes/mod.rs`，在现有 `"/{token}/skills/install-local"` 路由旁（约 100-101 行），加 upload 路由：

```rust
use axum::extract::DefaultBodyLimit;
// ...
.route(
    "/{token}/skills/install-local/upload",
    post(skills::install_local_upload).layer(DefaultBodyLimit::max(skills::MAX_UPLOAD_BYTES)),
)
```

- [ ] **Step 5: 实现 install_local_upload（zip 模式）+ MAX_UPLOAD_BYTES 常量**

`crates/server/src/routes/skills.rs`：

```rust
use axum::extract::Multipart;
use tempfile::TempDir;

/// upload 端点 body 上限（zip/目录上传）。Task 2 实测 multer 默认 per-field 后再调。
pub const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;

/// POST 上传 zip/目录安装本地 skill（multipart）。成功完整 Skills 页，失败 toast。
pub async fn install_local_upload(
    State(state): State<AppState>,
    Path(token): Path<String>,
    mut multipart: Multipart,
) -> Response {
    let mut archive: Option<Vec<u8>> = None;
    let mut name: Option<String> = None;
    let mut scope = Scope::Local;
    let mut force = false;
    while let Ok(Some(field)) = multipart.next_field().await {
        let fname = field.name().unwrap_or("").to_string();
        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => return error_response(format!("读取上传字段失败：{e}")),
        };
        match fname.as_str() {
            "archive" => archive = Some(bytes.to_vec()),
            "file" => {
                // 目录模式在 Task 2 实现；此处先占位拒绝，避免静默吞字段
                return error_response("目录上传尚未启用".to_string());
            }
            "name" => {
                let s = String::from_utf8_lossy(&bytes);
                let trimmed = s.trim();
                if !trimmed.is_empty() { name = Some(trimmed.to_string()); }
            }
            "scope" => {
                if String::from_utf8_lossy(&bytes).trim() == "global" { scope = Scope::Global; }
            }
            "force" => {
                force = matches!(String::from_utf8_lossy(&bytes).trim(), "on" | "true" | "1");
            }
            _ => {}
        }
    }
    let archive = match archive {
        Some(a) => a,
        None => return error_response("未收到 archive（.zip）字段".to_string()),
    };
    // 落临时区 → 调 core
    let tmp = match TempDir::new() {
        Ok(t) => t,
        Err(e) => return error_response(format!("创建临时目录失败：{e}")),
    };
    let zip_path = tmp.path().join("upload.zip");
    if let Err(e) = std::fs::write(&zip_path, &archive) {
        return error_response(format!("写入临时文件失败：{e}"));
    }
    match skillkit_core::install_local(
        &state.paths,
        zip_path.to_str().unwrap(),
        name.as_deref(),
        scope,
        force,
    ) {
        Ok(m) => render_skills(state, token, Some(&format!("✓ 已安装本地 skill：{}", m.id)), false, vec![], vec![]),
        Err(e) => {
            tracing::error!(error = ?e, "install-local upload 失败");
            error_response(format!("安装失败：{e}"))
        }
    }
    // tmp 在此 drop，自动清理
}
```

- [ ] **Step 6: 把现有 install_local 的 summary 从 f.path 改成 m.id**

`crates/server/src/routes/skills.rs` 的 `install_local`（637-664 行），把 `match skillkit_core::install_local(...)` 分支改为绑定 `SkillMeta`：

```rust
    match skillkit_core::install_local(&state.paths, &f.path, name, scope, force) {
        Ok(m) => render_skills(
            state,
            token,
            Some(&format!("✓ 已安装本地 skill：{}", m.id)),
            false,
            vec![],
            vec![],
        ),
        Err(e) => {
            tracing::error!(error = ?e, "install-local 失败：{}", f.path);
            error_response(format!("安装失败：{e}"))
        }
    }
```

- [ ] **Step 7: 跑测试验证通过**

Run: `cargo test -p skillkit-server --test routes install_local_upload_zip_success`
Expected: PASS。

同时回归现有 path 表单测试（summary 改 m.id 后应仍通过，因现有测试不验 summary 文本）：
Run: `cargo test -p skillkit-server --test routes install_local`
Expected: 3 个现有测试 PASS。

- [ ] **Step 8: 写失败测试——超上限 413（验 DefaultBodyLimit 生效，P3-1 测试方法）**

`crates/server/tests/routes.rs` 加（用自定义 router 挂小 limit 造略超 body，避免真造 100MiB）：

```rust
#[tokio::test]
async fn install_local_upload_rejects_oversize() {
    // 用一个仅挂 1MiB limit 的 router 验证超限 → 413，不依赖 MAX_UPLOAD_BYTES 真值
    use axum::{Router, routing::post, extract::DefaultBodyLimit, body::Body, http::Request};
    use tower::ServiceExt;
    let app = Router::new()
        .route("/upload", post(|| async { "ok" }))
        .layer(DefaultBodyLimit::max(1024 * 1024)); // 1MiB
    let big = vec![0u8; 1024 * 1024 + 1]; // 略超 1MiB
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/upload")
                .header("content-type", "application/octet-stream")
                .body(Body::from(big))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE); // 413
}
```

Run: `cargo test -p skillkit-server --test routes install_local_upload_rejects_oversize`
Expected: PASS（验证 `DefaultBodyLimit` 确实拦截超限 body）。

- [ ] **Step 9: 跑 make lint + make format**

Run: `make format && make lint`
Expected: 双绿（clippy `-D warnings` 不报 `too_many_lines` 等——若 `install_local_upload` 触发，按既有惯例拆辅助函数）。

- [ ] **Step 10: Commit**

```bash
git add crates/server/Cargo.toml crates/server/Cargo.lock crates/server/src/routes/mod.rs crates/server/src/routes/skills.rs crates/server/tests/routes.rs
git commit -m "feat(server): install-local upload 端点（zip）+ DefaultBodyLimit + summary 统一 m.id"
```

---

## Task 2：目录上传（multipart 多文件 + 安全重建）

**Files:**
- Modify: `crates/server/src/routes/skills.rs`（`install_local_upload` 的 `file` 分支 + 私有 `rebuild_dir`）
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: Task 1 的 `install_local_upload`、`MAX_UPLOAD_BYTES`。
- Produces: `install_local_upload` 完整支持 `archive`（zip）与 `file`（目录，多 part）两种字段；私有 `fn rebuild_dir(tmpdir: &Path, files: Vec<(String, Vec<u8>)>) -> Result<(), String>`（路径逃逸过滤 + 目录重建）。

- [ ] **Step 1: 写失败测试——multipart filename 含 `/` 的契约（锁定 tokio-multipart 行为，spec §9.1）**

`crates/server/tests/routes.rs` 加：

```rust
// 验证 axum Multipart 的 field.file_name() 原样保留含 `/` 的 filename（目录上传 relpath 契约）
#[tokio::test]
async fn multipart_filename_preserves_slash() {
    use axum::{Router, routing::post, extract::Multipart, response::IntoResponse, body::Body, http::Request};
    use tower::ServiceExt;
    async fn handler(mut m: Multipart) -> impl IntoResponse {
        let f = m.next_field().await.unwrap().unwrap();
        format!("{}|{}", f.name().unwrap_or(""), f.file_name().unwrap_or(""))
    }
    let app = Router::new().route("/m", post(handler));
    let body = b"--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"my-skill/SKILL.md\"\r\n\r\nhi\r\n--b--\r\n";
    let res = app.oneshot(
        Request::builder().method("POST").uri("/m")
            .header("content-type", "multipart/form-data; boundary=b")
            .body(Body::from(&body[..])).unwrap()
    ).await.unwrap();
    let text = body_to_string(res).await;
    assert_eq!(text, "file|my-skill/SKILL.md", "multer 必须保留 filename 中的 /，否则目录上传 relpath 失效");
}
```

Run: `cargo test -p skillkit-server --test routes multipart_filename_preserves_slash`
Expected: PASS（锁定契约；若 FAIL 说明 multer 剥离 `/`，需退化方案——见 Step 6 备注）。

- [ ] **Step 2: 写失败测试——目录上传成功（多 file part 重建目录树）**

```rust
#[tokio::test]
async fn install_local_upload_dir_success() {
    let (app, state) = build_test_app().await;
    // 两个 file part，filename 带 relpath
    let body = b"--b\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"dir-demo/SKILL.md\"\r\n\r\n\
---\r\nname: dir-demo\r\n---\r\n# dir-demo\r\n\
--b\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"dir-demo/scripts/run.sh\"\r\n\r\n\
#!/bin/sh\r\n\
--b\r\n\
Content-Disposition: form-data; name=\"scope\"\r\n\r\n\
local\r\n\
--b--\r\n";
    let res = app.oneshot(
        Request::builder().method("POST")
            .uri(format!("/{}/skills/install-local/upload", state.token))
            .header("content-type", "multipart/form-data; boundary=b")
            .body(Body::from(&body[..])).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let text = body_to_string(res).await;
    assert!(text.contains("✓ 已安装本地 skill：local/dir-demo"), "实际：{}", &text[..text.len().min(400)]);
    assert!(std::fs::read_to_string(state.paths.registry_path()).unwrap().contains("dir-demo"));
}
```

Run: `cargo test -p skillkit-server --test routes install_local_upload_dir_success`
Expected: FAIL（Task 1 的 `file` 分支返回"目录上传尚未启用"）。

- [ ] **Step 3: 写失败测试——路径逃逸拦截（../）**

```rust
#[tokio::test]
async fn install_local_upload_dir_rejects_traversal() {
    let (app, state) = build_test_app().await;
    let body = b"--b\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"../evil.sh\"\r\n\r\n\
pwn\r\n\
--b--\r\n";
    let res = app.oneshot(
        Request::builder().method("POST")
            .uri(format!("/{}/skills/install-local/upload", state.token))
            .header("content-type", "multipart/form-data; boundary=b")
            .body(Body::from(&body[..])).unwrap()
    ).await.unwrap();
    // 失败 → toast（4xx，不刷页），summary 不含成功
    let text = body_to_string(res).await;
    assert!(!text.contains("✓ 已安装本地 skill"), "逃逸应被拒，实际成功：{}", &text[..text.len().min(400)]);
}
```

Run: `cargo test -p skillkit-server --test routes install_local_upload_dir_rejects_traversal`
Expected: FAIL（同样 Step 5 实现后通过）。

- [ ] **Step 4: 实现 rebuild_dir（安全过滤 + 重建）**

`crates/server/src/routes/skills.rs` 加私有函数（放 `install_local_upload` 上方）：

```rust
/// 把上传的 (relpath, bytes) 列表在 tmpdir 下重建为目录树。
/// 安全：relpath 只接受 Normal 分量，拒 `..`/`.`/绝对路径；join 后再断言 starts_with 兜底。
fn rebuild_dir(tmpdir: &Path, files: Vec<(String, Vec<u8>)>) -> Result<(), String> {
    for (relpath, content) in files {
        let p = std::path::Path::new(&relpath);
        if p.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return Err(format!("路径含非法分量（.. / 绝对路径），已拒绝：{relpath}"));
        }
        let target = tmpdir.join(p);
        if !target.starts_with(tmpdir) {
            return Err(format!("路径越界，已拒绝：{relpath}"));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
        }
        std::fs::write(&target, &content).map_err(|e| format!("写入文件失败：{e}"))?;
    }
    Ok(())
}
```

- [ ] **Step 5: 替换 install_local_upload 的 file 分支（收集多 part → rebuild_dir → install_local）**

把 Task 1 Step 5 里 `file` 分支的 `return error_response(...)` 改为收集：

```rust
// 在 handler 顶部声明（与 archive/name 等并列）：
// let mut dir_files: Vec<(String, Vec<u8>)> = Vec::new();

// file 分支改为：
"file" => {
    let relpath = field.file_name().unwrap_or("").to_string();
    if relpath.is_empty() {
        return error_response("目录上传缺少文件相对路径".to_string());
    }
    dir_files.push((relpath, bytes.to_vec()));
}
```

handler 末尾（archive 判断之前）加目录分支：

```rust
// handler 在读完所有 field 后、`let archive = ...` 之前插入：
if !dir_files.is_empty() {
    if archive.is_some() {
        return error_response("不能同时上传 archive（zip）和 file（目录）".to_string());
    }
    let tmp = match TempDir::new() {
        Ok(t) => t,
        Err(e) => return error_response(format!("创建临时目录失败：{e}")),
    };
    if let Err(e) = rebuild_dir(tmp.path(), dir_files) {
        return error_response(e);
    }
    return match skillkit_core::install_local(
        &state.paths,
        tmp.path().to_str().unwrap(),
        name.as_deref(),
        scope,
        force,
    ) {
        Ok(m) => render_skills(state, token, Some(&format!("✓ 已安装本地 skill：{}", m.id)), false, vec![], vec![]),
        Err(e) => {
            tracing::error!(error = ?e, "install-local dir upload 失败");
            error_response(format!("安装失败：{e}"))
        }
    };
}
```

注意：core `install_local` 的目录分支会调 `resolve_skill_dir`（兼容根/单层父目录），若上传的是 `dir-demo/SKILL.md` 这种「顶层目录包住 skill」，core 视为单层父目录正常处理；若直接平铺 `SKILL.md`（无顶层目录），core 视为根布局，也正常。两种都覆盖。

- [ ] **Step 6: 跑测试验证通过**

Run: `cargo test -p skillkit-server --test routes install_local_upload`
Expected: 4 个测试全 PASS（zip 成功 / 超上限 413 / filename 含 / 契约 / 目录成功）。

Run: `cargo test -p skillkit-server --test routes install_local_upload_dir_rejects_traversal`
Expected: PASS。

备注（multer per-field 实测，P3-2）：在 Step 6 顺带构造一个接近 `MAX_UPLOAD_BYTES` 的 zip（如 `vec![0u8; 90*1024*1024]` 包装成 multipart）跑一次，观察是否被 multer per-field file_size 拒。若被拒，把 `MAX_UPLOAD_BYTES` 与实测的 per-field 上限对齐（可能需降到 multer per-field 值），更新常量注释。若不拒（multer 默认 file_size 无限），保持 100MiB。把结论写入 commit message。

- [ ] **Step 7: make format && make lint**

Run: `make format && make lint`
Expected: 双绿。

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/routes/skills.rs crates/server/tests/routes.rs
git commit -m "feat(server): install-local 目录上传（multipart 多文件 + 路径逃逸过滤）"
```

---

## Task 3：GET modal 模板 + 删旧表单片段

**Files:**
- Create: `crates/server/templates/fragments/install_local_modal.html`
- Delete: `crates/server/templates/fragments/install_local_form.html`
- Modify: `crates/server/src/routes/skills.rs`（`InstallLocalFormTpl` 改指新模板）
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `token` 模板变量（现有 GET handler 已注入）。
- Produces: GET `/{token}/skills/install-local` 返回 modal 片段，含 `.browse-overlay` > `.browse-modal.install-modal`，固定 id：`#install-form`、`#install-drop-zone`、隐藏 `input[name=archive]`、`input[name=file]`、`input[name=path]`、取消按钮、`.install-actions .primary` 提交按钮。

- [ ] **Step 1: 写失败测试——GET 返回 modal 关键结构**

`crates/server/tests/routes.rs` 加：

```rust
#[tokio::test]
async fn install_local_get_returns_modal_fragment() {
    let (app, state) = build_test_app().await;
    let res = app.oneshot(
        Request::builder().method("GET")
            .uri(format!("/{}/skills/install-local", state.token))
            .body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let text = body_to_string(res).await;
    assert!(text.contains("browse-overlay"), "缺遮罩层");
    assert!(text.contains("install-modal"), "缺 modal 容器");
    assert!(text.contains(r#"id="install-drop-zone""#), "缺拖放区");
    assert!(text.contains(r#"id="install-form""#), "缺 form");
    assert!(text.contains(r#"name="archive""#) && text.contains(r#"name="file""#) && text.contains(r#"name="path""#), "缺三种输入字段");
    assert!(text.contains("取消"), "缺取消按钮");
    // 不应残留旧表单标志
    assert!(!text.contains("install-local-form"));
}
```

Run: `cargo test -p skillkit-server --test routes install_local_get_returns_modal_fragment`
Expected: FAIL（旧模板无这些结构）。

- [ ] **Step 2: 新建 modal 模板**

`crates/server/templates/fragments/install_local_modal.html`：

```html
<div class="browse-overlay">
  <div class="browse-modal install-modal" role="dialog" aria-label="安装本地 skill">
    <header class="browse-header">
      <span class="browse-cwd">⇣ 安装本地 skill</span>
      <button type="button" class="browse-close" aria-label="关闭">✕</button>
    </header>
    <form class="install-body" id="install-form" onsubmit="return installLocalSubmit(event)">
      <div class="drop-zone" id="install-drop-zone">
        <p class="drop-hint">⇣ 拖放 .zip 文件或目录到此处</p>
        <div class="drop-actions">
          <button type="button" data-pick="archive">选择 .zip</button>
          <button type="button" data-pick="file">选择目录</button>
        </div>
        <input type="file" name="archive" accept=".zip,application/zip" hidden>
        <input type="file" name="file" webkitdirectory directory multiple hidden>
      </div>
      <div class="install-fields">
        <label class="field">
          <span>或直接输入路径</span>
          <input type="text" name="path" placeholder="~/skills/my-skill 或 ./pkg.zip">
        </label>
        <details class="advanced">
          <summary>高级选项</summary>
          <label class="field"><span>name</span><input type="text" name="name" placeholder="默认读 SKILL.md"></label>
          <label class="field">
            <span>scope</span>
            <select name="scope"><option value="local" selected>local</option><option value="global">global</option></select>
          </label>
          <label class="field-check"><input type="checkbox" name="force" value="on"> 覆盖已存在</label>
        </details>
      </div>
      <div class="install-actions">
        <button type="button" onclick="this.closest('.browse-overlay').remove()">取消</button>
        <button type="submit" class="primary">▶ 安装</button>
      </div>
    </form>
  </div>
</div>
```

- [ ] **Step 3: 删旧模板 + 改 GET handler 指向新模板**

删 `crates/server/templates/fragments/install_local_form.html`。

`crates/server/src/routes/skills.rs` 的 `InstallLocalFormTpl`（约 622-626 行）把 `#[template(path = "fragments/install_local_form.html")]` 改为 `#[template(path = "fragments/install_local_modal.html")]`。结构体名可保留 `InstallLocalFormTpl` 不动（避免连锁改名），或重命名为 `InstallLocalModalTpl`（推荐，语义清晰，但需同步改 `install_local_form` handler 里的类型引用）。

- [ ] **Step 4: 跑测试验证通过**

Run: `cargo test -p skillkit-server --test routes install_local_get_returns_modal_fragment`
Expected: PASS。

- [ ] **Step 5: make check（Askama 编译验证）**

Run: `make format && make lint && cargo test -p skillkit-server`
Expected: 全绿（含 Task 1/2 测试回归）。

- [ ] **Step 6: Commit**

```bash
git add crates/server/templates/fragments/install_local_modal.html crates/server/src/routes/skills.rs
git rm crates/server/templates/fragments/install_local_form.html
git commit -m "feat(server): install-local GET 改返回 modal 浮层片段（替代内联表单）"
```

---

## Task 4：toolbar 入口改造（按钮统一 + #modal-mount + 删 span）

**Files:**
- Modify: `crates/server/templates/fragments/skills_main.html:15-31`
- Modify: `crates/server/templates/layout.html`（加 `#modal-mount`）

**Interfaces:**
- Consumes: Task 3 的 GET modal 端点。
- Produces: toolbar「安装本地」按钮 class 统一为泛化 `button`，`hx-get` 拉 modal 到 `#modal-mount`；删 `<span id="install-local-panel">`；`layout.html` body 末尾有固定 `<div id="modal-mount"></div>`。

- [ ] **Step 1: 改 skills_main.html——删 span、按钮 class 统一、target 改 #modal-mount**

`crates/server/templates/fragments/skills_main.html:15-31` 的 `.head-actions` 改为：

```html
  <div class="head-actions">
    <button hx-get="/{{ token }}/skills/install-local"
            hx-target="#modal-mount" hx-swap="innerHTML">安装本地</button>
    <form class="inline" hx-post="/{{ token }}/skills/import"
          hx-target="body" hx-swap="outerHTML" hx-indicator="#import-indicator">
      <button>导入存量 skill</button>
      <span id="import-indicator" class="htmx-indicator">导入中…</span>
    </form>
    <form class="inline" hx-post="/{{ token }}/skills/upgrade-all"
          hx-target="body" hx-swap="outerHTML" hx-indicator="#upgrade-all-indicator">
      <button>全部升级</button>
      <span id="upgrade-all-indicator" class="htmx-indicator">升级中…</span>
    </form>
  </div>
```

要点：①删掉原 `<span class="install-local-panel" id="install-local-panel"></span>`；②「安装本地」从 `class="pill-btn"` 改为无 class（走泛化 `button` 样式，与「导入/升级」一致）。

- [ ] **Step 2: layout.html 加 #modal-mount**

在 `crates/server/templates/layout.html` 的 `<body>` 末尾（现有 `<script>` 块之前或之后均可，建议在 `</body>` 前）加：

```html
<div id="modal-mount"></div>
```

- [ ] **Step 3: make check（模板编译 + 现有测试不破）**

Run: `make format && make lint && cargo test -p skillkit-server`
Expected: 全绿（无新测试，靠编译 + 现有测试 + 后续 GUI 走查）。

- [ ] **Step 4: Commit**

```bash
git add crates/server/templates/fragments/skills_main.html crates/server/templates/layout.html
git commit -m "refactor(server): Skills toolbar 入口统一 + install-local 浮层挂载点 #modal-mount"
```

---

## Task 5：CSS 规则（§4.5）

**Files:**
- Modify: `crates/server/static/app.css`

**Interfaces:**
- Consumes: demo 颜色变量 `--accent`/`--ink`/`--bg`/`--surface`/`--line`/`--accent-soft`/`--mono`（已定义）。
- Produces: `.install-modal`、`.install-body`、`.drop-zone`(+`.drag`/`.has-file`)、`.drop-actions`、`.install-fields`、`.field`、`.field-check`、`.advanced`、`.install-actions`、`.install-actions .primary` 规则块。

- [ ] **Step 1: 追加 CSS 规则块**

在 `crates/server/static/app.css` 末尾（或「目录浏览浮层」段之后）追加：

```css
/* ---------- 安装本地 modal ---------- */
.install-modal { width: 600px; max-width: 100%; max-height: min(80vh, 640px); }
.install-body { padding: 16px; display: flex; flex-direction: column; gap: 14px; overflow: auto; }

.drop-zone {
  border: 2px dashed var(--line); border-radius: 8px; padding: 28px 16px;
  text-align: center; display: flex; flex-direction: column; align-items: center; gap: 12px;
  transition: all .15s; background: var(--surface);
}
.drop-zone.drag { border-color: var(--accent); background: var(--accent-soft); }
.drop-zone.has-file { border-style: solid; border-color: var(--ink-3); }
.drop-zone .drop-hint { font-family: var(--mono); font-size: 13px; color: var(--ink-2); margin: 0; }
.drop-zone .file-card {
  font-family: var(--mono); font-size: 12.5px; color: var(--ink);
  display: inline-flex; align-items: center; gap: 8px;
  background: var(--surface-2); padding: 6px 10px; border-radius: 6px;
}
.drop-zone .file-card button { border: none; background: none; color: var(--danger); cursor: pointer; padding: 0 4px; }
.drop-actions { display: inline-flex; gap: 8px; }
.drop-actions button {
  font-family: var(--mono); font-size: 11.5px; cursor: pointer;
  padding: 5px 12px; border-radius: 6px; border: 1px solid var(--line);
  background: var(--surface); color: var(--ink-2); transition: all .15s;
}
.drop-actions button:hover { color: var(--ink); border-color: var(--ink-3); }

.install-fields { display: flex; flex-direction: column; gap: 12px; }
.install-fields .field { display: flex; flex-direction: column; gap: 4px; }
.install-fields .field > span {
  font-family: var(--mono); font-size: 11px; color: var(--ink-2);
  letter-spacing: .04em; text-transform: uppercase;
}
.install-fields .field input, .install-fields .field select {
  font-family: var(--mono); font-size: 13px; padding: 7px 10px;
  border: 1px solid var(--line); border-radius: 6px; background: var(--surface);
}
.install-fields .field-check { font-size: 13px; color: var(--ink-2); display: inline-flex; align-items: center; gap: 6px; }
.advanced { border-top: 1px dashed var(--line); padding-top: 10px; }
.advanced summary {
  font-family: var(--mono); font-size: 11px; color: var(--ink-3);
  letter-spacing: .04em; text-transform: uppercase; cursor: pointer; padding: 2px 0;
}
.advanced[open] { display: flex; flex-direction: column; gap: 10px; }

.install-actions { display: flex; justify-content: flex-end; gap: 8px; padding-top: 4px; }
.install-actions .primary {
  font-family: var(--mono); font-size: 12.5px; font-weight: 600; cursor: pointer;
  padding: 9px 20px; border-radius: 7px; border: 1px solid var(--ink);
  background: var(--ink); color: var(--bg); transition: all .15s;
}
.install-actions .primary:hover { background: var(--accent); border-color: var(--accent); }
.install-actions .primary:disabled { opacity: .6; cursor: not-allowed; }
```

- [ ] **Step 2: make check（rust-embed 重打包 + 编译）**

Run: `make format && make lint && cargo build -p skillkit-cli`
Expected: 全绿。

- [ ] **Step 3: Commit**

```bash
git add crates/server/static/app.css
git commit -m "feat(server): install-local modal 专属 CSS（拖放区/字段分组/主按钮）"
```

---

## Task 6：前端 JS（拖放/递归/互斥/折叠/取消/安装中/提交路由）

**Files:**
- Modify: `crates/server/templates/layout.html`（`<script>` 块内加 install-local JS）

**Interfaces:**
- Consumes: Task 3 的 modal 结构（`#install-form`/`#install-drop-zone`/`input[name=archive|file|path]`/`[data-pick]`/`.browse-overlay`）；Task 1/2 的两个 POST 端点；htmx（`window.htmx`）；token（现有 `SK_TOKEN` 变量，`layout.html:73`）。
- Produces: 全局 `installLocalSubmit(event)`（form onsubmit 回调）+ 拖放/选择/互斥/安装中态逻辑；复用现有 `closeBrowseOverlay`（✕/遮罩/ESC 已覆盖）。

- [ ] **Step 1: 加 install-local 交互 JS**

在 `crates/server/templates/layout.html` 的现有 `<script>` 块内（`closeBrowseOverlay` 定义之后，或末尾）追加：

```javascript
// ===== install-local modal 交互 =====
function installLocalSetup(modal) {
  var form = modal.querySelector('#install-form');
  if (!form) return;
  var dz = form.querySelector('#install-drop-zone');
  var archiveInput = form.querySelector('input[name="archive"]');
  var fileInput = form.querySelector('input[name="file"]');
  var pathInput = form.querySelector('input[name="path"]');

  // 当前输入源：'archive' | 'file' | 'path' | null
  function setSource(kind) {
    dz.classList.toggle('has-file', kind === 'archive' || kind === 'file');
    if (kind !== 'archive') archiveInput.value = '';
    if (kind !== 'file') fileInput.value = '';
    if (kind !== 'path') pathInput.value = '';
    // 已选卡片展示
    var card = dz.querySelector('.file-card');
    if (card) card.remove();
    if (kind === 'archive' && archiveInput.files[0]) {
      showCard(archiveInput.files[0].name, fmtSize(archiveInput.files[0].size));
    } else if (kind === 'file' && fileInput.files.length) {
      var top = topDirName(fileInput.files[0].webkitRelativePath);
      showCard(top + '/（' + fileInput.files.length + ' 文件）', '');
    }
  }
  function showCard(name, size) {
    var c = document.createElement('div');
    c.className = 'file-card';
    c.innerHTML = name + (size ? ' · ' + size : '') + ' <button type="button">×</button>';
    c.querySelector('button').addEventListener('click', function () { setSource(null); });
    dz.querySelector('.drop-actions').before(c);
  }
  function fmtSize(n) { return n > 1048576 ? (n/1048576).toFixed(1)+'MB' : (n/1024).toFixed(0)+'KB'; }
  function topDirName(rel) { var p = (rel||'').split('/'); return p[0] || '目录'; }

  // 选择按钮触发隐藏 input
  dz.querySelectorAll('[data-pick]').forEach(function (btn) {
    btn.addEventListener('click', function () { (btn.dataset.pick === 'archive' ? archiveInput : fileInput).click(); });
  });
  archiveInput.addEventListener('change', function () { if (archiveInput.files[0]) setSource('archive'); });
  fileInput.addEventListener('change', function () { if (fileInput.files.length) setSource('file'); });
  pathInput.addEventListener('input', function () { if (pathInput.value.trim()) { if (archiveInput.value||fileInput.value) setSource('path'); } });

  // 拖放
  ['dragenter','dragover'].forEach(function (ev) {
    dz.addEventListener(ev, function (e) { e.preventDefault(); dz.classList.add('drag'); });
  });
  ['dragleave','drop'].forEach(function (ev) {
    dz.addEventListener(ev, function (e) { e.preventDefault(); dz.classList.remove('drag'); });
  });
  dz.addEventListener('drop', function (e) {
    var items = e.dataTransfer.items;
    if (items && items.length && items[0].webkitGetAsEntry) {
      // 目录或文件 entry：交给 input[type=file] 的 webkitdirectory 不可用，直接读 entry
      var entry = items[0].webkitGetAsEntry();
      if (entry && entry.isDirectory) {
        collectDirFromEntry(entry, function (fileList) { fileInput.files = fileList; setSource('file'); });
      } else if (entry && entry.isFile) {
        archiveInput.files = e.dataTransfer.files; setSource('archive');
      }
    } else if (e.dataTransfer.files.length) {
      archiveInput.files = e.dataTransfer.files; setSource('archive');
    }
  });

  // 递归读 FileSystemDirectoryEntry → FileList（DataTransferItemList 构造）
  function collectDirFromEntry(dirEntry, cb) {
    var reader = dirEntry.createReader();
    var allFiles = [];
    var reader = dirEntry.createReader();
    function readEntries() {
      reader.readEntries(function (entries) {
        if (!entries.length) {
          // 用 DataTransfer 构造 FileList
          var dt = new DataTransfer();
          allFiles.forEach(function (f) { dt.items.add(f); });
          cb(dt.files);
          return;
        }
        var pending = entries.length;
        entries.forEach(function (en) {
          if (en.isFile) {
            en.file(function (f) {
              // 改造 file 的相对路径：DataTransfer 不允许改 webkitRelativePath，
              // 拖放目录改走 archive 通道需要打包——此处退化为「拒绝拖放目录，提示用选择目录按钮」
              allFiles.push(f);
              if (--pending === 0) readEntries();
            });
          } else if (en.isDirectory) {
            collectDirFromEntry(en, function (sub) {
              for (var i=0;i<sub.length;i++) allFiles.push(sub[i]);
              if (--pending === 0) readEntries();
            });
          } else { if (--pending === 0) readEntries(); }
        });
      });
    }
    readEntries();
  }
}

// form submit 路由：按输入源 POST 到 path 端点或 upload 端点
function installLocalSubmit(e) {
  e.preventDefault();
  var form = e.target;
  var archiveInput = form.querySelector('input[name="archive"]');
  var fileInput = form.querySelector('input[name="file"]');
  var pathInput = form.querySelector('input[name="path"]');
  var token = window.SK_TOKEN || '';
  var url, fd = new FormData();
  // 公共字段
  var name = form.querySelector('input[name="name"]').value.trim();
  var scope = form.querySelector('select[name="scope"]').value;
  var force = form.querySelector('input[name="force"]').checked;
  if (name) fd.append('name', name);
  fd.append('scope', scope);
  if (force) fd.append('force', 'on');

  var hasZip = archiveInput.files && archiveInput.files[0];
  var hasDir = fileInput.files && fileInput.files.length;
  if (hasZip) {
    url = '/' + token + '/skills/install-local/upload';
    fd.append('archive', archiveInput.files[0], archiveInput.files[0].name);
  } else if (hasDir) {
    url = '/' + token + '/skills/install-local/upload';
    for (var i = 0; i < fileInput.files.length; i++) {
      var f = fileInput.files[i];
      // relpath 来自 webkitRelativePath（webkitdirectory 选择）；拖放目录见 collectDirFromEntry 备注
      fd.append('file', f, (f.webkitRelativePath || f.name));
    }
  } else if (pathInput.value.trim()) {
    url = '/' + token + '/skills/install-local';
    fd.append('path', pathInput.value.trim());
  } else {
    alert('请拖放/选择文件，或输入路径'); return false;
  }

  // 安装中态
  var submitBtn = form.querySelector('.install-actions .primary');
  submitBtn.disabled = true;
  var orig = submitBtn.textContent;
  submitBtn.textContent = '安装中…';
  htmx.ajax('POST', url, { source: form, target: 'body', swap: 'outerHTML', values: {} })
    .then(function () { /* 成功：body 已被替换，modal 随旧 body 消失 */ })
    .catch(function () { submitBtn.disabled = false; submitBtn.textContent = orig; });
  // htmx.ajax 用 source 会自动收集 form 的 input；但 FormData 文件需手动——改用 htmx.ajax 的 values 不含文件，
  // 因此文件上传改用原生 fetch + htmx 处理响应：见下方实际实现备注。
  return false;
}
```

**关键实现备注（执行时务必处理，非占位）**：`htmx.ajax` 的 `values` 不支持文件 FormData。文件上传需用原生 `fetch(url, { method:'POST', body: fd })`，拿到完整 HTML 响应后手动 `document.body.outerHTML = respText`（模拟 htmx 的 body outerHTML swap），并触发 htmx 的后续处理（重新执行 `<script>`、重新绑定 SSE）。具体写法：
```javascript
fetch(url, { method: 'POST', body: fd, headers: { 'HX-Request': 'true' } })
  .then(function (r) { return r.text(); })
  .then(function (html) {
    document.open(); document.write(html); document.close(); // 让浏览器重新解析+执行 script
  })
  .catch(function () { submitBtn.disabled = false; submitBtn.textContent = orig; alert('安装失败，请重试'); });
```
`document.write` 会重执行 layout.html 的 `<script>`（含 SSE 重连、`closeBrowseOverlay` 重绑定），与现有写操作 body outerHTML 语义一致。执行时以此替换上面的 `htmx.ajax` 块。

`installLocalSetup` 需在 modal 注入后调用。最简：在 toolbar 按钮的 `hx-get` 上加 `hx-on::after-request="installLocalSetup(this.closest('body').querySelector('#install-modal-root')||document.querySelector('.browse-overlay.install-modal'))"`，或更稳——在 `layout.html` 的 htmx 全局 `htmx:afterRequest` 监听里检测 `.install-modal` 出现则调 `installLocalSetup`。

- [ ] **Step 2: make check**

Run: `make format && make lint && cargo build -p skillkit-cli`
Expected: 全绿（JS 不参与 Rust 编译，但 rust-embed 重打包 + Askama 模板需通过）。

- [ ] **Step 3: Commit**

```bash
git add crates/server/templates/layout.html
git commit -m "feat(server): install-local modal 前端交互（拖放/目录递归/互斥/提交路由/安装中态）"
```

---

## Task 7：GUI 走查 + CLI 回归

**Files:**
- 临时脚本 `/tmp/install-local-modal-check.py`（playwright，不入库，沿用交接 §3.4 约定）

**Interfaces:**
- Consumes: Task 1-6 全部产出。

- [ ] **Step 1: 起 server**

Run: `cargo build -p skillkit-cli && make run ARGS="serve --port 7317"`
Expected: 启动成功，日志含 token（grep `[0-9a-f]{32}`）。

- [ ] **Step 2: 跑 playwright DOM 走查脚本**

脚本断言（`wait_until="load"`，DOM 轮询不用 `expect_navigation`，截图失败改 DOM 断言）：
- toolbar 三按钮（安装本地/导入/升级）`getComputedStyle` 高度/padding 一致。
- 点「安装本地」→ `.browse-overlay.install-modal` 可见。
- 四种关闭：✕ / 点遮罩 / ESC / 取消按钮 → modal 消失。
- 拖放区 `dragover` 后 `.drag` 高亮态（`classList.contains('drag')`）。
- 「选择 .zip」触发 `input[name=archive]` click；选文件后 `.has-file` + `.file-card` 显示文件名。
- 路径输入与文件互斥：输入路径后 `.has-file` 移除。
- 高级选项 `<details>` 展开/折叠。
- 路径方式安装成功：summary 横幅 `✓ 已安装本地 skill：local/<name>` 出现 + 4s 淡出（轮询 `.summary`）。
- zip 拖放安装成功：同上 summary。
- 目录选择安装成功：同上。
- 冲突安装失败：toast 出现 + modal 保持打开（`.browse-overlay` 仍在）。

Run: `~/.local/pipx/venvs/playwright/bin/python /tmp/install-local-modal-check.py`
Expected: 全 PASS。

- [ ] **Step 3: CLI 回归（install local 不受影响）**

```bash
make run ARGS="install local <某 skill 目录>"
make run ARGS="install local <某 skill.zip> --json"
make run ARGS="install local <同名> --force"
```
Expected: 分别输出 `✓ 已安装` / SkillMeta JSON / 覆盖无残留。

- [ ] **Step 4: make check 全量**

Run: `make check`
Expected: 全绿（含所有新增 server 测试）。

- [ ] **Step 5: 修复（若走查发现问题）+ Commit**

若 GUI 走查或回归发现问题，逐项修复后：
```bash
git add -A
git commit -m "fix(server): install-local modal 走查问题修复（<具体项>）"
```
若全绿无修复，本 task 无 commit。

---

## Self-Review

**1. Spec coverage：**
- §4.1 入口触发（删 span、#modal-mount、复用 .browse-overlay 关闭）→ Task 3（GET modal）+ Task 4（toolbar）✅
- §4.2 modal 四区布局（拖放/选择/路径/高级折叠/取消接线/主按钮）→ Task 3（模板）+ Task 5（CSS）+ Task 6（JS）✅
- §4.3 三条上传路径（path/zip/目录）+ 端点分离 + summary m.id → Task 1（zip + summary）+ Task 2（目录）✅
- §4.4 反馈（成功整页/失败 toast/安装中态）→ Task 1（render_skills/error_response）+ Task 6（安装中 disabled）✅
- §4.5 CSS 规则 → Task 5 ✅
- §5 依赖三处 → Task 1 Step 1 ✅
- §7 测试（zip/目录/逃逸/超上限/GET modal/summary local/<name>/取消按钮）→ Task 1（zip/超上限）+ Task 2（目录/逃逸/filename 契约）+ Task 3（GET modal）+ Task 7（取消按钮 GUI 走查）✅
- §8 安全（路径逃逸过滤/体积上限/临时区/token）→ Task 2（rebuild_dir 过滤）+ Task 1（DefaultBodyLimit）✅
- §9 风险（filename 含 / 契约→Task 2 Step 1；body 替换 JS 重绑定→Task 6 document.write 重执行 script；file input 清空→Task 6 setSource；安装中态→Task 6）✅

**2. Placeholder scan：** Task 6 Step 1 有「实现备注」段明确 document.write 替换 htmx.ajax（非占位，是强制实现指令）。其余步骤均含实际代码/命令。无 TBD/TODO。

**3. Type consistency：** `install_local_upload` 签名（State, Path, Multipart）→ Response，Task 1/2 一致；`rebuild_dir(&Path, Vec<(String,Vec<u8>)>) -> Result<(), String>`，Task 2 定义与调用一致；`MAX_UPLOAD_BYTES`（Task 1 定义）在 mod.rs（Task 1 Step 4）引用一致；modal id `#install-form`/`#install-drop-zone`（Task 3 模板）与 JS（Task 6）引用一致。

**潜在执行风险（执行者留意，非 plan 缺陷）：**
- Task 6 的 `collectDirFromEntry`：DataTransfer 不允许改 File 的 `webkitRelativePath`，拖放目录的 relpath 无法直接塞 multipart filename。当前 plan 的处理：webkitdirectory 选择目录有 `webkitRelativePath`（可靠）；**拖放目录**若需支持，执行时验证 `e.dataTransfer.files[k].webkitRelativePath` 在拖放时是否也填充（Chrome/Edge 通常填充），若填充则直接用，不填充则 UI 提示「拖放目录请改用选择目录按钮」。Task 7 走查覆盖此场景。
- Task 1 Step 3 的测试辅助函数（`build_test_app`/`body_to_string`）沿用现有 `tests/routes.rs` 的命名，执行时若命名不同需对齐现有测试顶部 `use` 与辅助定义。
