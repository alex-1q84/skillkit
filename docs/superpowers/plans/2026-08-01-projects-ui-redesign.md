# Projects 管理界面重构 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Projects 详情页重构为「绑定 profile → 应用」单一心智模型（去手动勾选、status 顶部横向、重绑定路径向导），列表页补注册/扫描分区与项目注销。

**Architecture:** core 加 `Project::set_profiles`（替换语义重算 installed_skills 并集）+ `project::remove`（注销）；server 删 3 旧端点（set_skills/apply_profile/apply）加 2 新端点（profiles 绑定+落地、DELETE 注销），统一写操作返回完整页 body outerHTML；详情页改四块布局 + status badge + profile 卡片多选，列表页改两张分区卡片 + 删除按钮。

**Tech Stack:** Rust 2021 + Axum 0.8 + Askama + htmx + `form_urlencoded`（重复 key 收集）+ `tempfile`（集成测试）。

## Global Constraints

- 路径绝不硬编码：home 兜底用 `dirs::home_dir()`，不写死 `/Users/...`（CLAUDE.md §7）。
- server 薄壳：业务逻辑（重算 installed_skills、落地）只在 core，handler 只编排（CLAUDE.md §3/§5）。
- 写操作（POST/DELETE）一律返回完整页 `hx-target="body" hx-swap="outerHTML"`，不用 `HX-Redirect`（frontend-rules §1，避 SSE 竞态）。
- 片段外层固定 id：`status-panel` 等，htmx 替换后 id 不丢（frontend-rules §1）。
- 重复 key 表单（`profiles=a&profiles=b`）用 `form_urlencoded::parse(&body).filter(...)` 手动收集（serde_urlencoded 不支持，frontend-rules §6）。
- handler 预计算展示数据（`ProfileCard` / `local_skills` / `ProjectRow`），不在模板里调方法（askama 方法借用参数坑，frontend-rules §4-10）。
- 改完每个 task 跑 `make check`（format + lint + test）双绿后 commit；commit message 中文 + Conventional Commits。
- core 公开类型在 `lib.rs` re-export；新增 `Project::set_profiles`/`project::remove` 是 pub，已随 `Project`/模块导出，无需改 lib.rs。

## File Structure

**core（1 改）**
- Modify: `crates/core/src/project.rs` — 加 `set_profiles` 方法 + `remove` 函数 + 单元测试

**server routes（2 改）**
- Modify: `crates/server/src/routes/projects.rs` — 删 3 旧 handler/结构体、加 2 新 handler、`Workspace*Tpl`/`Projects*Tpl` 字段重构、`render_workspace`/`render_list` 预算展示数据、加 `ProfileCard`/`ProjectRow` 结构体
- Modify: `crates/server/src/routes/mod.rs` — 删 3 旧路由、加 2 新路由、`{id}` 路由合并 `get().delete()`

**模板（3 改 1 删）**
- Modify: `crates/server/templates/fragments/workspace_main.html` — 四块布局重写
- Modify: `crates/server/templates/fragments/status.html` — 纵向 pre → 横向 badge 条
- Modify: `crates/server/templates/fragments/projects_main.html` — 分区卡片 + 删除按钮 + local 计数
- Delete: `crates/server/templates/fragments/apply_result.html` — 死代码（apply handler 删除）

**静态（1 改）**
- Modify: `crates/server/static/app.css` — workspace 块状布局 + status badge + profile 卡片

**测试（2 改）**
- Modify: `crates/server/tests/routes.rs` — 删 3 旧测试、加 set_profiles/remove/渲染测试
- Modify: `e2e/test_ui.py` — 选择器更新（若依赖旧按钮文本/status 形态）

---

### Task 1: core — `set_profiles` + `remove`

**Files:**
- Modify: `crates/core/src/project.rs`（加方法 + 函数 + 3 个单元测试）

**Interfaces:**
- Produces: `Project::set_profiles(&mut self, names: &[String], profiles: &[crate::profile::Profile])`；`pub fn remove(paths: &Paths, id: &str) -> Result<()>`

- [ ] **Step 1: 写失败的单元测试**

在 `crates/core/src/project.rs` 的 `mod tests` 内（`scan_projects_finds_git_dirs_with_depth_limit` 测试之后、`}` 闭合 tests 模块之前）追加：

```rust
    #[test]
    fn set_profiles_recomputes_union_and_replaces() {
        let mut proj = Project {
            id: "X1".into(),
            name: "p".into(),
            path: "/tmp/p".into(),
            agents: vec![],
            applied_profiles: vec!["old".into()],
            installed_skills: vec!["old/x".into()],
            locked_shas: BTreeMap::new(),
        };
        let fe = crate::profile::Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec!["dc/a".into(), "dc/b".into()],
        };
        let base = crate::profile::Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec!["dc/b".into(), "dc/c".into()], // b 与 fe 重叠
        };
        proj.set_profiles(&["fe".into(), "base".into()], &[fe, base]);
        assert_eq!(
            proj.applied_profiles,
            vec!["fe".to_string(), "base".to_string()],
            "applied_profiles 替换为所选"
        );
        assert_eq!(
            proj.installed_skills,
            vec!["dc/a".to_string(), "dc/b".to_string(), "dc/c".to_string()],
            "installed_skills = 并集去重保序，旧值被替换"
        );
    }

    #[test]
    fn set_profiles_replace_unbinds_previous() {
        let mut proj = Project {
            id: "X2".into(),
            name: "p".into(),
            path: "/tmp/p".into(),
            agents: vec![],
            applied_profiles: vec!["fe".into()],
            installed_skills: vec!["dc/a".into()],
            locked_shas: BTreeMap::new(),
        };
        let base = crate::profile::Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec!["dc/z".into()],
        };
        // 改绑只剩 base：fe 的 skill 应被清除（替换语义，可取消绑定）
        proj.set_profiles(&["base".into()], &[base]);
        assert_eq!(proj.applied_profiles, vec!["base".to_string()]);
        assert_eq!(
            proj.installed_skills,
            vec!["dc/z".to_string()],
            "取消 fe 绑定后其 skill 不再保留"
        );
    }

    #[test]
    fn remove_deletes_toml_and_errors_when_missing() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        Project {
            id: "RM1".into(),
            name: "p".into(),
            path: "/tmp/p".into(),
            agents: vec![],
            applied_profiles: vec![],
            installed_skills: vec![],
            locked_shas: BTreeMap::new(),
        }
        .save(&paths)
        .unwrap();
        assert!(paths.projects_dir().join("RM1.toml").exists());
        Project::remove(&paths, "RM1").unwrap();
        assert!(!paths.projects_dir().join("RM1.toml").exists());
        assert!(matches!(
            Project::remove(&paths, "RM1"),
            Err(SkillkitError::ProjectNotFound { .. })
        ));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-core set_profiles 2>&1 | tail -15` 和 `cargo test -p skillkit-core remove_deletes 2>&1 | tail -15`
Expected: 编译失败（`set_profiles` / `remove` 未定义）。

- [ ] **Step 3: 实现 `set_profiles` 方法 + `remove` 函数**

在 `crates/core/src/project.rs` 的 `impl Project { ... }` 内（`apply_profile` 方法之后）追加：

```rust
    /// 设定绑定 profile 集合（替换语义）+ 重算 installed_skills 为所选 profiles 的 skills 并集（去重保序）。
    /// names 中找不到对应 profile 的条目静默跳过（handler 应先校验存在性并给可读 err）。
    pub fn set_profiles(&mut self, names: &[String], profiles: &[crate::profile::Profile]) {
        self.applied_profiles = names.to_vec();
        let mut skills: Vec<String> = Vec::new();
        for name in names {
            if let Some(p) = profiles.iter().find(|p| &p.name == name) {
                for id in &p.skills {
                    if !skills.contains(id) {
                        skills.push(id.clone());
                    }
                }
            }
        }
        self.installed_skills = skills;
    }
```

在 `impl Project { ... }` 闭合之后、`pub fn list_ids` 之前追加独立函数：

```rust
/// 注销项目：删 ~/.skillkit/projects/<id>.toml。不存在返回 ProjectNotFound。
/// 只删元数据，不碰项目目录任何文件（已落地 symlink 保留，shared/git 资产绝不动）。
pub fn remove(paths: &Paths, id: &str) -> Result<()> {
    let path = paths.projects_dir().join(format!("{id}.toml"));
    if !path.exists() {
        return Err(SkillkitError::ProjectNotFound { id: id.to_string() });
    }
    std::fs::remove_file(&path)?;
    Ok(())
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p skillkit-core set_profiles 2>&1 | tail -15` 和 `cargo test -p skillkit-core remove_deletes 2>&1 | tail -15`
Expected: 3 个测试全 PASS。

- [ ] **Step 5: `make check` 双绿 + commit**

```bash
make check
git add crates/core/src/project.rs
git commit -m "feat(core): Project set_profiles 替换重算 + remove 注销——profile 绑定驱动基础"
```

---

### Task 2: 详情页后端 + 渲染重构（删旧端点 + 字段 + 模板）

> 内聚一个 task：详情页后端（删 3 旧 handler/结构体 + Workspace 字段重构 + render_workspace 预算）与渲染（workspace_main/status 重写）一次到位，避免中间态 use churn。set_profiles/remove handler 在 Task 3/4 加。

**Files:**
- Modify: `crates/server/src/routes/projects.rs`
- Modify: `crates/server/src/routes/mod.rs`（删 3 旧路由）
- Modify: `crates/server/templates/fragments/workspace_main.html`
- Modify: `crates/server/templates/fragments/status.html`
- Delete: `crates/server/templates/fragments/apply_result.html`
- Test: `crates/server/tests/routes.rs`（删 3 旧测试 + 加渲染测试）

**Interfaces:**
- Consumes: Task 1 的 `Project::set_profiles`（Task 3 用）、`Scope`（`skillkit_core::Scope`）
- Produces: `ProfileCard { name, skill_count, bound }`；`render_workspace(state, token, proj, fragment, report: Option<ApplyReport>)`（report 参数，Task 3 set_profiles 传 Some）；`Workspace*Tpl` 新字段

- [ ] **Step 1: 删 3 旧测试**

在 `crates/server/tests/routes.rs` 删除这 3 个测试函数（连同其前的 `#[tokio::test]` 行）：
- `project_set_skills_replaces_installed`（测 `POST /projects/{id}/skills`）
- `project_apply_lands_symlink`（测 `POST /projects/{id}/apply`）
- `projects_apply_profile_merges_skills`（测 `POST /projects/{id}/apply-profile`）

- [ ] **Step 2: 加失败的详情页渲染测试**

在 `crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn workspace_renders_status_badge_profile_cards_and_local_only() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // registry：1 local + 1 global
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "dc/local".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/local".into(),
            name: "local".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Local,
            version: None,
            computed_hash: Some("s1".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: dir.path().join("canon/local").to_string_lossy().into_owned(),
        },
    );
    reg.skills.insert(
        "dc/glob".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/glob".into(),
            name: "glob".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Global,
            version: None,
            computed_hash: Some("s2".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: dir.path().join("canon/glob").to_string_lossy().into_owned(),
        },
    );
    reg.save(&state.paths).unwrap();
    // profile fe
    skillkit_core::Profile {
        name: "fe".into(),
        description: String::new(),
        skills: vec!["dc/local".into()],
    }
    .save(&state.paths)
    .unwrap();
    // project：绑了 fe，installed 含 local + global
    let proj_root = dir.path().join("p");
    std::fs::create_dir_all(&proj_root).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "p".into(),
        path: proj_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec!["fe".into()],
        installed_skills: vec!["dc/local".into(), "dc/glob".into()],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/projects/ABCDEF12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("status-panel"), "含 status badge 条");
    assert!(body.contains("已同步"), "全同步显已同步");
    assert!(body.contains("绑定: fe") || body.contains("绑定：fe"), "展示绑定 profiles");
    // profile 卡片：fe 已绑预选
    assert!(body.contains(r#"name="profiles""#), "profile 卡片是 checkbox");
    assert!(body.contains("fe"), "列出 profile fe");
    // local 区块只列 local，不含 global
    assert!(body.contains("dc/local"), "local 区块含 local skill");
    assert!(
        body.contains("local installed skills"),
        "local 区块标题"
    );
    // 旧手动勾选表单已去掉
    assert!(
        !body.contains(r#"name="skills""#),
        "不再有手动勾选 skill 的 checkbox"
    );
}
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -p skillkit-server workspace_renders_status_badge 2>&1 | tail -20`
Expected: 编译失败（旧模板/字段还在，新断言不通过）。

- [ ] **Step 4: 改 `projects.rs` — 删旧 handler/结构体 + 调整 use + 加 ProfileCard + 重构 Workspace 字段 + render_workspace**

`crates/server/src/routes/projects.rs`：

(a) 顶部 `use skillkit_core::{...}` 改为（去 `run_apply`/`SkillMeta`——Task 2 后无引用；`ApplyReport` 留作 report 字段；加 `Scope`）：

```rust
use skillkit_core::{
    build_status, compute_diff, scan_shared, ApplyDiff, ApplyReport, Project, Registry, Scope,
    StatusView,
};
```

(b) 删除 `ApplyResultTpl` 结构体（原 `#[template(path = "fragments/apply_result.html")]`，约 65-71 行）。

(c) 删除 `ApplyProfileForm` 结构体（约 194-197 行）。

(d) `WorkspaceTpl` 与 `WorkspaceMainTpl` 字段统一改为（去 `all_skills`，加 `local_skills`/`profiles`/`report`）：

```rust
#[derive(Template)]
#[template(path = "project_workspace.html")]
pub struct WorkspaceTpl<'a> {
    pub token: &'a str,
    pub project: &'a Project,
    pub status: StatusView,
    pub shared: Vec<String>,
    pub local_skills: Vec<String>,
    pub profiles: Vec<ProfileCard>,
    pub report: Option<ApplyReport>,
}

/// 纯 main 内容片段（工作台 SSE 刷新用），不含 nav。字段与 WorkspaceTpl 一致。
#[derive(Template)]
#[template(path = "fragments/workspace_main.html")]
pub struct WorkspaceMainTpl<'a> {
    pub token: &'a str,
    pub project: &'a Project,
    pub status: StatusView,
    pub shared: Vec<String>,
    pub local_skills: Vec<String>,
    pub profiles: Vec<ProfileCard>,
    pub report: Option<ApplyReport>,
}
```

(e) 在 `WorkspaceMainTpl` 之后加 `ProfileCard` 结构体（非 Template，纯展示数据）：

```rust
/// profile 卡片展示数据（handler 预计算，避免模板调方法）。
pub struct ProfileCard {
    pub name: String,
    pub skill_count: usize,
    pub bound: bool,
}
```

(f) 删除 `apply_profile` handler（约 200-216 行）。

(g) 删除 `set_skills` handler（约 247-264 行）。

(h) 删除 `apply` handler（约 278-302 行）。

(i) `workspace` handler 调 `render_workspace` 时传 `None`（report）：

```rust
pub async fn workspace(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
    Query(q): Query<FragmentQuery>,
) -> Response {
    let Ok(proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    render_workspace(state, token, proj, q.is_fragment(), None)
}
```

(j) `render_workspace` 改签名加 `report` 参数 + 预算 `local_skills`/`profiles`：

```rust
fn render_workspace(
    state: AppState,
    token: String,
    proj: Project,
    fragment: bool,
    report: Option<ApplyReport>,
) -> Response {
    let reg = Registry::load(&state.paths).unwrap_or_default();
    let diff = compute_diff(&proj, &reg).unwrap_or_else(|_| ApplyDiff {
        expected: vec![],
        conflicts: vec![],
    });
    let status = build_status(&state.paths, &proj, &diff).unwrap_or(StatusView {
        expected: vec![],
        missing: vec![],
        extra: vec![],
        conflicts: vec![],
    });
    let shared = scan_shared(StdPath::new(&proj.path), &proj.agents);
    let local_skills: Vec<String> = proj
        .installed_skills
        .iter()
        .filter(|id| {
            reg.get(*id)
                .map(|m| m.scope == Scope::Local)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    let profiles: Vec<ProfileCard> = skillkit_core::list_profile_names(&state.paths)
        .unwrap_or_default()
        .into_iter()
        .map(|name| {
            let skill_count = skillkit_core::Profile::load(&state.paths, &name)
                .map(|p| p.skills.len())
                .unwrap_or(0);
            let bound = proj.applied_profiles.iter().any(|n| n == &name);
            ProfileCard {
                name,
                skill_count,
                bound,
            }
        })
        .collect();
    let rendered = if fragment {
        WorkspaceMainTpl {
            token: &token,
            project: &proj,
            status,
            shared,
            local_skills,
            profiles,
            report,
        }
        .render()
    } else {
        WorkspaceTpl {
            token: &token,
            project: &proj,
            status,
            shared,
            local_skills,
            profiles,
            report,
        }
        .render()
    };
    render_str(rendered)
}
```

(k) `rebind` handler 末尾仍调 `render_workspace`，补 `None`：

```rust
    render_workspace(state, token, proj, false, None)
```

- [ ] **Step 5: 删 `apply_result.html`**

```bash
rm crates/server/templates/fragments/apply_result.html
```

- [ ] **Step 6: 改 `mod.rs` — 删 3 旧路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，删除这 3 行：
```rust
        .route(
            "/{token}/projects/{id}/apply-profile",
            post(projects::apply_profile),
        )
        .route("/{token}/projects/{id}/skills", post(projects::set_skills))
        .route("/{token}/projects/{id}/apply", post(projects::apply))
```

（`apply_profile`/`set_skills`/`apply` handler 已删，路由必须同步删，否则编译错。）

- [ ] **Step 7: 重写 `workspace_main.html`**

整个文件替换为：

```html
<h1>{{ project.name }}
  <button class="x"
          hx-delete="/{{ token }}/projects/{{ project.id }}"
          hx-target="body" hx-swap="outerHTML"
          hx-confirm="注销后该项目不再被 skillkit 管理，已落地文件保留。确定？">删除</button>
</h1>
<p>{{ project.path }} · agents: {{ project.agents.join(", ") }}{% if !project.applied_profiles.is_empty() %} · 绑定: {{ project.applied_profiles.join(", ") }}{% endif %}</p>

{% include "fragments/status.html" %}

<section class="card">
  <h2>重绑定路径</h2>
  <p class="hint">项目移动/改名后，用浏览向导修正路径。</p>
  <form class="inline" hx-post="/{{ token }}/projects/{{ project.id }}/rebind"
        hx-target="body" hx-swap="outerHTML">
    <input id="path" name="path" type="text" placeholder="新路径" required value="{{ project.path }}">
    <button type="button"
            hx-get="/{{ token }}/projects/browse?into=path&panel=browse-panel-rebind"
            hx-target="#browse-panel-rebind"
            hx-include="#path">浏览...</button>
    <button>重绑定</button>
  </form>
  <div id="browse-panel-rebind"></div>
</section>

<section class="card">
  <h2>绑定 Profile</h2>
  {% if profiles.is_empty() %}
  <p class="hint">还没有 profile，去 <a href="/{{ token }}/profiles">Profiles 视图</a>创建。</p>
  {% else %}
  <form hx-post="/{{ token }}/projects/{{ project.id }}/profiles"
        hx-target="body" hx-swap="outerHTML">
    <div class="profile-grid">
      {% for p in &profiles %}
      <label class="profile-card{% if p.bound %} bound{% endif %}">
        <input type="checkbox" name="profiles" value="{{ p.name }}"{% if p.bound %} checked{% endif %}>
        <span class="pc-name">{{ p.name }}</span>
        <span class="pc-count">{{ p.skill_count }} skills</span>
      </label>
      {% endfor %}
    </div>
    <button class="apply">▶ 应用</button>
  </form>
  {% endif %}
  {% if let Some(ref r) = report %}
  <div class="apply-result">
    <p class="hint">上次应用：created {{ r.created.len() }} · removed {{ r.removed.len() }} · recopied {{ r.recopied.len() }}{% if !r.warnings.is_empty() %} · ⚠ {{ r.warnings.len() }} warnings{% endif %}</p>
    {% for w in &r.warnings %}<p class="warn">{{ w }}</p>{% endfor %}
  </div>
  {% endif %}
</section>

<div class="workspace-bottom">
  <section class="col">
    <h2>local installed skills ({{ local_skills.len() }})</h2>
    <ul>{% for id in &local_skills %}<li>{{ id }}</li>{% else %}<li>—</li>{% endfor %}</ul>
  </section>
  <section class="col">
    <h2>shared（只读 · git 管）</h2>
    <ul>{% for s in &shared %}<li>{{ s }}</li>{% else %}<li>—</li>{% endfor %}</ul>
  </section>
</div>
```

- [ ] **Step 8: 改 `status.html` — 纵向 pre → 横向 badge 条**

整个文件替换为：

```html
<div id="status-panel" class="status-bar">
  {% if status.missing.is_empty() && status.extra.is_empty() && status.conflicts.is_empty() %}
  <span class="badge ok">✓ 已同步 · {{ status.expected.len() }} expected</span>
  {% else %}
  <span class="badge">expected {{ status.expected.len() }}</span>
  {% if !status.missing.is_empty() %}
  <details class="badge warn"><summary>missing {{ status.missing.len() }}</summary>
    <ul>{% for m in &status.missing %}<li>{{ m }}</li>{% endfor %}</ul>
  </details>
  {% endif %}
  {% if !status.extra.is_empty() %}
  <details class="badge warn"><summary>extra {{ status.extra.len() }}</summary>
    <ul>{% for m in &status.extra %}<li>{{ m }}</li>{% endfor %}</ul>
  </details>
  {% endif %}
  {% if !status.conflicts.is_empty() %}
  <details class="badge danger"><summary>conflicts {{ status.conflicts.len() }}</summary>
    <ul>{% for m in &status.conflicts %}<li>{{ m }}</li>{% endfor %}</ul>
  </details>
  {% endif %}
  {% endif %}
</div>
```

- [ ] **Step 9: 跑测试确认通过**

Run: `cargo test -p skillkit-server workspace_renders_status_badge 2>&1 | tail -20`
Expected: PASS。

- [ ] **Step 10: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/projects.rs crates/server/src/routes/mod.rs crates/server/templates/fragments/workspace_main.html crates/server/templates/fragments/status.html crates/server/tests/routes.rs
git rm crates/server/templates/fragments/apply_result.html
git commit -m "refactor(gui): 项目详情页重构——status badge+profile 卡片+local 过滤，删 3 旧端点"
```

Expected: `make check` 双绿（删 handler 后 `run_apply`/`SkillMeta` 暂未用，Step 4 的 use 调整已处理；Task 3 加回 `run_apply`）。

---

### Task 3: `set_profiles` handler（绑定 + 落地一步到位）

**Files:**
- Modify: `crates/server/src/routes/projects.rs`（加 `set_profiles` handler + use 加回 `run_apply`）
- Modify: `crates/server/src/routes/mod.rs`（加 `POST /{token}/projects/{id}/profiles`）
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: Task 1 的 `Project::set_profiles`；Task 2 的 `render_workspace(..., Some(report))`
- Produces: `projects::set_profiles` handler

- [ ] **Step 1: 写失败的集成测试**

在 `crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn project_set_profiles_binds_lands_and_reports() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // registry：1 local skill（带 canonical 目录，供落地）
    let canon = dir.path().join(".skillkit/.agents/skills/logseq");
    std::fs::create_dir_all(&canon).unwrap();
    std::fs::write(canon.join("SKILL.md"), "x").unwrap();
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "dc/logseq".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/logseq".into(),
            name: "logseq".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Local,
            version: None,
            computed_hash: Some("sha1".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        },
    );
    reg.save(&state.paths).unwrap();
    // profile fe 含 dc/logseq
    skillkit_core::Profile {
        name: "fe".into(),
        description: String::new(),
        skills: vec!["dc/logseq".into()],
    }
    .save(&state.paths)
    .unwrap();
    // project（需 .git/info 供落地写 exclude）
    let proj_root = dir.path().join("proj");
    std::fs::create_dir_all(proj_root.join(".git/info")).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "proj".into(),
        path: proj_root.to_string_lossy().into_owned(),
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
                .uri("/test-token/projects/ABCDEF12/profiles")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("profiles=fe"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // 绑定 + 重算 installed_skills
    let after = skillkit_core::Project::load(&state.paths, "ABCDEF12").unwrap();
    assert_eq!(after.applied_profiles, vec!["fe".to_string()]);
    assert_eq!(after.installed_skills, vec!["dc/logseq".to_string()]);
    // 落地：symlink 建出
    assert!(
        proj_root.join(".claude/skills/logseq").is_symlink(),
        "set_profiles 应一步落地建 symlink"
    );
    // 响应是完整工作台页（含 report 区，写操作返回完整页）
    let body = common::body_string(resp).await;
    assert!(body.contains("上次应用"), "响应含落地结果区");
    assert!(body.contains("status-panel"), "响应含 status");
}

#[tokio::test]
async fn project_set_profiles_unknown_profile_returns_hint() {
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

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/profiles")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("profiles=nope"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("不存在"), "未知 profile 给可读提示，不 500");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server project_set_profiles 2>&1 | tail -20`
Expected: 失败（`/projects/{id}/profiles` 路由 404）。

- [ ] **Step 3: 实现 `set_profiles` handler**

`crates/server/src/routes/projects.rs`：

(a) 顶部 `use skillkit_core::{...}` 加回 `run_apply`（Step 4 of Task 2 移除过）：

```rust
use skillkit_core::{
    build_status, compute_diff, run_apply, scan_shared, ApplyDiff, ApplyReport, Project, Registry,
    Scope, StatusView,
};
```

(b) 在 `rebind` handler 之后加 `set_profiles` handler（重复 key 用 `form_urlencoded::parse` 收集，同原 set_skills 模式）：

```rust
/// 设定 profile 绑定（替换语义）+ 重算 installed_skills + 落地，一步到位。
/// 返回完整工作台页（含落地报告）。未知 profile 给可读 err 片段，不 500。
pub async fn set_profiles(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let names: Vec<String> = parse(&body)
        .filter(|(k, _)| k.as_ref() == "profiles")
        .map(|(_, v)| v.into_owned())
        .collect();
    let Ok(mut proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // load 所选 profiles；任一不存在给可读 err
    let mut profiles = Vec::new();
    for name in &names {
        match skillkit_core::Profile::load(&state.paths, name) {
            Ok(p) => profiles.push(p),
            Err(_) => {
                return Html(
                    r#"<p class="err">profile 不存在，先去 <a href="/"#.to_string()
                        + &token
                        + r#"/profiles">Profiles 视图</a>创建。</p>"#,
                )
                .into_response();
            }
        }
    }
    proj.set_profiles(&names, &profiles);
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let report = match run_apply(&state.paths, &mut proj, false) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::error!(error = ?e, "set_profiles 落地失败");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    // 落地可能更新 locked_shas，再存一次
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_workspace(state, token, proj, false, report)
}
```

- [ ] **Step 4: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()`，在 `/{token}/projects/{id}/rebind` 之后加：

```rust
        .route("/{token}/projects/{id}/profiles", post(projects::set_profiles))
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p skillkit-server project_set_profiles 2>&1 | tail -20`
Expected: 2 个测试 PASS。

- [ ] **Step 6: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/projects.rs crates/server/src/routes/mod.rs crates/server/tests/routes.rs
git commit -m "feat(gui): 项目绑定 profile 多选应用——绑定+重算+落地一步到位"
```

---

### Task 4: `remove` handler + DELETE 路由合并

**Files:**
- Modify: `crates/server/src/routes/projects.rs`（加 `remove` handler）
- Modify: `crates/server/src/routes/mod.rs`（`{id}` 路由合并 `get().delete()`）
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: Task 1 的 `project::remove`
- Produces: `projects::remove` handler

- [ ] **Step 1: 写失败的集成测试**

在 `crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn project_remove_deletes_and_returns_list_page() {
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
    assert!(state.paths.projects_dir().join("ABCDEF12.toml").exists());

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/test-token/projects/ABCDEF12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        !state.paths.projects_dir().join("ABCDEF12.toml").exists(),
        "toml 已删"
    );
    // 返回完整 Projects 列表页（写操作返回完整页 body outerHTML）
    let body = common::body_string(resp).await;
    assert!(body.contains("Projects"), "返回列表页");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server project_remove_deletes 2>&1 | tail -20`
Expected: 失败（DELETE `/projects/{id}` 路由未注册）。

- [ ] **Step 3: 实现 `remove` handler**

`crates/server/src/routes/projects.rs`，在 `list` handler 之前加（用当前 `render_list(token, projects, fragment)` 签名，不改签名——Task 5 才改）：

```rust
/// 注销项目：删 toml（不碰项目目录），返回完整 Projects 列表页（写操作返回完整页）。
pub async fn remove(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    if skillkit_core::project::remove(&state.paths, &id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut projects = Vec::new();
    if let Ok(ids) = skillkit_core::list_project_ids(&state.paths) {
        for pid in ids {
            if let Ok(p) = Project::load(&state.paths, &pid) {
                projects.push(p);
            }
        }
    }
    render_list(token, projects, false)
}
```

（`skillkit_core::project::remove` 走全路径：`lib.rs` 只 re-export 了 `Project`/`list_project_ids`/`scan_projects`，`remove` 函数通过 `pub mod project` 可达。本 task 不改 `render_list` 签名。）

- [ ] **Step 4: 路由合并**

`crates/server/src/routes/mod.rs`：把原 `/{token}/projects/{id}` 的 `get(workspace)` 合并 `delete(remove)`：

```rust
        .route(
            "/{token}/projects/{id}",
            get(projects::workspace).delete(projects::remove),
        )
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p skillkit-server project_remove_deletes 2>&1 | tail -20`
Expected: PASS。

- [ ] **Step 6: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/projects.rs crates/server/src/routes/mod.rs crates/server/tests/routes.rs
git commit -m "feat(gui): 项目注销端点——DELETE toml 返回列表页，与 sources/skills 同款"
```

---

### Task 5: 列表页重构（分区卡片 + 删除按钮 + local 计数）

**Files:**
- Modify: `crates/server/src/routes/projects.rs`（`Projects*Tpl` 加 `rows: Vec<ProjectRow>` + `render_list` 预算 local_count + 加 `ProjectRow` 结构体）
- Modify: `crates/server/templates/fragments/projects_main.html`
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: Task 4 的 `render_list(state, ...)` 签名
- Produces: `ProjectRow { id, name, path, local_count }`

- [ ] **Step 1: 写失败的渲染测试**

在 `crates/server/tests/routes.rs` 末尾追加：

```rust
#[tokio::test]
async fn projects_list_renders_section_cards_delete_and_local_count() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // registry：1 local + 1 global
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "dc/local".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/local".into(),
            name: "local".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Local,
            version: None,
            computed_hash: Some("s".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: "/canon/local".into(),
        },
    );
    reg.skills.insert(
        "dc/glob".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/glob".into(),
            name: "glob".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Global,
            version: None,
            computed_hash: Some("s".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: "/canon/glob".into(),
        },
    );
    reg.save(&state.paths).unwrap();
    // project：installed 含 local + global
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "myapp".into(),
        path: "/tmp/myapp".into(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec!["dc/local".into(), "dc/glob".into()],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

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
    // 两张分区卡片
    assert!(body.contains("注册项目"), "注册卡片");
    assert!(body.contains("扫描发现"), "扫描卡片");
    // 删除按钮（hx-delete）
    assert!(
        body.contains("hx-delete=\"/test-token/projects/ABCDEF12\""),
        "列表项含删除按钮"
    );
    // local 计数：只算 local（1），不含 global
    assert!(body.contains("1 local skills"), "local skill 数过滤");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server projects_list_renders_section 2>&1 | tail -20`
Expected: FAIL（当前无分区卡片/删除按钮/local 计数）。

- [ ] **Step 3: 加 `ProjectRow` + 改 `Projects*Tpl` + `render_list` 预算**

`crates/server/src/routes/projects.rs`：

(a) `ProjectsTpl` 与 `ProjectsMainTpl` 字段把 `projects: Vec<Project>` 改为 `rows: Vec<ProjectRow>`：

```rust
#[derive(Template)]
#[template(path = "projects.html")]
pub struct ProjectsTpl<'a> {
    pub token: &'a str,
    pub rows: Vec<ProjectRow>,
}

#[derive(Template)]
#[template(path = "fragments/projects_main.html")]
pub struct ProjectsMainTpl<'a> {
    pub token: &'a str,
    pub rows: Vec<ProjectRow>,
}
```

(b) 在 `ProjectsMainTpl` 之后加 `ProjectRow`：

```rust
/// 列表项展示数据（handler 预计算 local_count，避免模板调 registry）。
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub local_count: usize,
}
```

(c) `render_list` 改签名接 `&AppState` 算 local_count，并同步所有调用点：

```rust
fn render_list(state: &AppState, token: String, projects: Vec<Project>, fragment: bool) -> Response {
    let reg = Registry::load(&state.paths).unwrap_or_default();
    let rows: Vec<ProjectRow> = projects
        .iter()
        .map(|p| {
            let local_count = p
                .installed_skills
                .iter()
                .filter(|id| {
                    reg.get(*id)
                        .map(|m| m.scope == Scope::Local)
                        .unwrap_or(false)
                })
                .count();
            ProjectRow {
                id: p.id.clone(),
                name: p.name.clone(),
                path: p.path.clone(),
                local_count,
            }
        })
        .collect();
    let rendered = if fragment {
        ProjectsMainTpl {
            token: &token,
            rows,
        }
        .render()
    } else {
        ProjectsTpl {
            token: &token,
            rows,
        }
        .render()
    };
    render_str(rendered)
}
```

调用点同步（签名从 `(token, projects, fragment)` → `(&state, token, projects, fragment)`，三个调用点都改）：
- `list` handler 末尾：`render_list(&state, token, projects, q.is_fragment())`
- `add` handler 末尾：`render_list(&state, token, projects, false)`
- `remove` handler（Task 4）末尾：`render_list(&state, token, projects, false)`

- [ ] **Step 4: 重写 `projects_main.html`**

整个文件替换为：

```html
<h1>Projects</h1>

<section class="card">
  <h2>注册项目</h2>
  <p class="hint">已知项目路径？直接填路径注册。</p>
  <form class="inline" hx-post="/{{ token }}/projects"
        hx-target="body" hx-swap="outerHTML">
    <input id="path" name="path" type="text" placeholder="项目绝对路径（如 /Users/me/app）" required>
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
    <input id="dir" name="dir" type="text" placeholder="扫描根目录（如 ~/code）" required>
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

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p skillkit-server projects_list_renders_section 2>&1 | tail -20`
Expected: PASS。

- [ ] **Step 6: `make check` 双绿 + commit**

```bash
make check
git add crates/server/src/routes/projects.rs crates/server/templates/fragments/projects_main.html crates/server/tests/routes.rs
git commit -m "feat(gui): Projects 列表页分区卡片+注销按钮+local 计数"
```

---

### Task 6: app.css — 块状布局 + status badge + profile 卡片

**Files:**
- Modify: `crates/server/static/app.css`

**Interfaces:**
- Consumes: Task 2/5 引入的 class：`status-bar` / `badge ok|warn|danger` / `profile-grid` / `profile-card.bound` / `pc-name` / `pc-count` / `workspace-bottom` / `project-list` / `apply-result`

- [ ] **Step 1: 追加样式**

在 `crates/server/static/app.css` 末尾追加（保留既有样式不动，仅新增 + 改 `.workspace` 相关）：

```css

/* ---------- 项目详情：status badge 条 ---------- */
.status-bar {
  display: flex; flex-wrap: wrap; align-items: center; gap: 8px;
  margin-bottom: 18px; padding: 12px 14px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 8px;
  box-shadow: var(--shadow);
}
.status-bar .badge { font-size: 11.5px; padding: 3px 10px; }
.status-bar .badge.ok { color: var(--ok); border-color: var(--ok); background: var(--surface); }
.status-bar details.badge { padding: 0; border: none; background: none; }
.status-bar details.badge > summary {
  font-family: var(--mono); font-size: 11.5px; font-weight: 600;
  color: var(--warn); cursor: pointer; padding: 3px 10px;
  border: 1px solid var(--line); border-radius: 999px; background: var(--surface-2);
  list-style: none;
}
.status-bar details.badge.danger > summary { color: var(--danger); }
.status-bar details.badge ul {
  list-style: none; margin-top: 6px; padding-left: 4px;
  font-family: var(--mono); font-size: 12px; color: var(--ink-2);
}
.status-bar details.badge ul li { padding: 2px 0; }

/* ---------- 项目详情：底部 local/shared 两列（取代原三栏 grid） ---------- */
.workspace-bottom {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 18px;
  margin-top: 18px;
  align-items: start;
}

/* ---------- 项目详情：profile 卡片网格 ---------- */
.profile-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 10px;
  margin-bottom: 14px;
}
.profile-card {
  display: flex; flex-direction: column; gap: 4px;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
  cursor: pointer;
  transition: all .12s;
}
.profile-card:hover { background: var(--surface-2); border-color: var(--ink-3); }
.profile-card input { position: absolute; opacity: 0; pointer-events: none; }
.profile-card.bound {
  border-color: var(--accent-2);
  background: var(--accent-soft);
  box-shadow: 0 0 0 1px var(--accent-2) inset;
}
.pc-name { font-family: var(--mono); font-size: 13px; font-weight: 600; color: var(--ink); }
.pc-count { font-family: var(--mono); font-size: 11px; color: var(--ink-3); }

/* ---------- 项目列表：行内布局 ---------- */
ul.project-list { list-style: none; }
ul.project-list li {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  padding: 10px 0; border-bottom: 1px solid var(--line);
}
ul.project-list li a { font-weight: 500; min-width: 120px; }
ul.project-list .muted { color: var(--ink-3); font-family: var(--mono); font-size: 12px; flex: 1; min-width: 200px; }
```

同时把原 `.workspace { ... grid-template-columns: repeat(3, 1fr); ... }`（约 204 行）整段删除——详情页不再用三栏 grid（Task 2 的 workspace_main 已不输出 `.workspace` 容器）。

- [ ] **Step 2: `make check` + `make e2e` 回归**

Run: `make check && make e2e 2>&1 | tail -30`
Expected: `make check` 双绿；`make e2e` 现有用例通过（若 e2e 选择器因 UI 变化失败，Task 7 修）。

- [ ] **Step 3: 手动走查（可选但推荐）**

```bash
make run ARGS="serve --port 7317"
```

打开浏览器走查：详情页 status 横向 badge、点 profile 卡片切换选中、点「应用」落地、重绑定浏览向导、列表页两张卡片 + 删除按钮。Ctrl-C 停。

- [ ] **Step 4: commit**

```bash
git add crates/server/static/app.css
git commit -m "style(gui): Projects 详情页块状布局+status badge+profile 卡片样式"
```

---

### Task 7: e2e 选择器更新 + 全量回归收尾

**Files:**
- Modify: `e2e/test_ui.py`（仅当现有用例依赖被删按钮文本「update」/「APPLY」或旧 status `pre` 形态时改）

- [ ] **Step 1: 跑 e2e 定位失败**

Run: `make e2e 2>&1 | tail -40`
Expected: 若有用例失败，记录失败点（选择器找按钮文本 / status 断言）。

- [ ] **Step 2: 按失败点更新 `e2e/test_ui.py`**

常见调整（按实际失败改，不要臆测）：
- 找「update」/「APPLY」按钮文本的断言 → 改为找「▶ 应用」或 `name="profiles"` checkbox。
- 断言 `pre.status` 文本 → 改为断言 `.status-bar` / `.badge`。
- 若有 Projects 详情页用例，补一条「选 profile 卡片 → 点应用 → 断言落地」可选用例（见 Step 3）。

- [ ] **Step 3:（可选）加详情页绑定→应用 e2e 用例**

若 `e2e/test_ui.py` 有 projects 详情页的测试基建，参照其模式加：

```python
def test_project_bind_profile_applies(page, base):
    # 进项目详情 → 勾选 profile 卡片 → 点应用 → 断言 local installed skills 出现
    ...
```

（无基建则跳过，集成测试 Task 3 已覆盖该路径。）

- [ ] **Step 4: 全量回归**

Run: `make check && make e2e && make e2e-cli 2>&1 | tail -40`
Expected: 全绿。`make e2e-cli` 需 npx（若环境无，跳过并在 commit message 注明）。

- [ ] **Step 5: commit**

```bash
git add e2e/test_ui.py
git commit -m "test(e2e): 适配 Projects 详情页重构后的选择器"
```

- [ ] **Step 6: 更新交接（可选）**

若本轮工作告一段落，在 `docs/sessions/2026-07-29-skillkit-design.md` 末尾追加一节交接状态（参照既有 §7 格式）：本次重构完成的 7 task、验证状态、后续待办（如 global skill 项目级展示、GUI/CLI 语义统一）。

---

## Self-Review 结论

**Spec 覆盖**：
- 决策 1（profile 驱动）→ Task 1 `set_profiles` + Task 2 去手动勾选 + Task 3 绑定应用
- 决策 2（应用一步到位）→ Task 3 `set_profiles`（绑定+重算+落地）
- 决策 3（删除只注销）→ Task 1 `remove` + Task 4 `remove` handler
- 派生：local scope 过滤 → Task 2 `local_skills` + Task 5 `local_count`
- 派生：替换语义 → Task 1 `set_profiles` 测试 `set_profiles_replace_unbinds_previous`
- §3 详情页布局（status badge/重绑定向导/profile 卡片/local+shared）→ Task 2 模板 + Task 6 CSS
- §4 列表页（分区卡片/删除按钮/local 计数）→ Task 5 + Task 6 CSS
- §5.2 端点增删 → Task 2/3/4
- §5.3 删 apply_result.html → Task 2 Step 5
- §5.5 CSS → Task 6
- §6 测试 → 各 task 内 TDD
- §7 限制（global 静默、报告区瞬时）→ Task 2/3 实现 + 注释
- review P0（DELETE 返回完整页）→ Task 4 `render_list`
- review P1（set_profiles 返回完整页）→ Task 3 `render_workspace(..., Some)`
- review P1（删 apply_result.html）→ Task 2
- review P2（路由合并）→ Task 4 Step 4
- review P2（报告区瞬时）→ spec §3.3 已注，Task 2 模板注释「上次应用」

无 spec 条目缺 task。

**占位扫描**：无 TBD/TODO；每步含完整代码或精确指引。

**类型一致**：
- `ProfileCard { name: String, skill_count: usize, bound: bool }`：Task 2 定义，workspace_main.html 用 `p.name`/`p.skill_count`/`p.bound` 一致。
- `ProjectRow { id, name, path, local_count }`：Task 5 定义，projects_main.html 用 `row.id`/`row.name`/`row.path`/`row.local_count` 一致。
- `render_workspace(state, token, proj, fragment, report)`：Task 2 定义 5 参，Task 2 workspace/rebind 传 `None`、Task 3 set_profiles 传 `Some(report)` 一致。
- `render_list(&AppState, token, projects, fragment)`：Task 5 首次改签名接 `&AppState` 算 local_count，同步 `list`/`add`/`remove` 三个调用点；Task 4 的 `remove` 仍用旧签名 `(token, projects, fragment)`（Task 5 改签名时一并更新）。
- `Project::set_profiles(&mut self, names: &[String], profiles: &[crate::profile::Profile])`：Task 1 定义，Task 3 `proj.set_profiles(&names, &profiles)` 一致。
- `project::remove(paths, id)`：Task 1 定义，Task 4 `skillkit_core::project::remove(&state.paths, &id)` 一致（注意 Task 4 用全路径 `skillkit_core::project::remove`，因 lib.rs 未单独 re-export `remove` 函数，只 re-export 了 `Project`/`list_ids`/`scan_projects`）。

**已知实现注意**：
- askama `{% if let Some(ref r) = report %}`（Task 2 workspace_main）：`report` 是拥有的 `Option<ApplyReport>` 字段，用 `ref` 借用绑定；askama 0.13 支持 `if let`。若编译报借用/move 错，改写为 `{% if let Some(r) = report.as_ref() %}`。
