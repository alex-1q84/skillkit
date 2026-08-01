//! Projects 视图：列表 + 工作台（声明编辑 + apply 闭环；shared 只读）。
use askama::Template;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use form_urlencoded::parse;
use serde::Deserialize;
use skillkit_core::{
    build_status, compute_diff, run_apply, scan_shared, ApplyDiff, ApplyReport, Project, Registry,
    SkillMeta, StatusView,
};
use std::path::{Path as StdPath, PathBuf};

use crate::routes::FragmentQuery;
use crate::AppState;

#[derive(Template)]
#[template(path = "projects.html")]
pub struct ProjectsTpl<'a> {
    pub token: &'a str,
    pub projects: Vec<Project>,
}

/// 纯 main 内容片段（SSE 刷新用），不含 nav。
#[derive(Template)]
#[template(path = "fragments/projects_main.html")]
pub struct ProjectsMainTpl<'a> {
    pub token: &'a str,
    pub projects: Vec<Project>,
}

#[derive(Template)]
#[template(path = "project_workspace.html")]
pub struct WorkspaceTpl<'a> {
    pub token: &'a str,
    pub project: &'a Project,
    pub status: StatusView,
    pub shared: Vec<String>,
    /// (meta, 是否已在 installed_skills)——工作台勾选预置 checked。
    pub all_skills: Vec<(SkillMeta, bool)>,
    pub profiles: Vec<String>,
}

/// 纯 main 内容片段（工作台 SSE 刷新用），不含 nav。
#[derive(Template)]
#[template(path = "fragments/workspace_main.html")]
pub struct WorkspaceMainTpl<'a> {
    pub token: &'a str,
    pub project: &'a Project,
    pub status: StatusView,
    pub shared: Vec<String>,
    /// (meta, 是否已在 installed_skills)——工作台勾选预置 checked。
    pub all_skills: Vec<(SkillMeta, bool)>,
    pub profiles: Vec<String>,
}

#[derive(Template)]
#[template(path = "fragments/status.html")]
pub struct StatusTpl {
    pub status: StatusView,
}

#[derive(Template)]
#[template(path = "fragments/apply_result.html")]
pub struct ApplyResultTpl<'a> {
    pub token: &'a str,
    pub project_id: &'a str,
    pub report: ApplyReport,
}

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
        _ => skillkit_core::config::Config::load(&state.paths)
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
    State(_state): State<AppState>,
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
            render_str(rendered)
        }
        Err(e) => {
            tracing::error!(error = ?e, "scan 失败：{}", f.dir);
            Html("<p class=\"err\">扫描失败，检查目录路径</p>").into_response()
        }
    }
}

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
    let Ok(profile) = skillkit_core::Profile::load(&state.paths, &f.profile) else {
        return Html("<p class=\"err\">profile 不存在</p>").into_response();
    };
    proj.apply_profile(&f.profile, &profile.skills);
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    status_fragment(state, proj)
}

pub async fn list(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<FragmentQuery>,
) -> Response {
    let mut projects = Vec::new();
    if let Ok(ids) = skillkit_core::list_project_ids(&state.paths) {
        for id in ids {
            if let Ok(p) = Project::load(&state.paths, &id) {
                projects.push(p);
            }
        }
    }
    render_list(token, projects, q.is_fragment())
}

pub async fn workspace(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
    Query(q): Query<FragmentQuery>,
) -> Response {
    let Ok(proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    render_workspace(state, token, proj, q.is_fragment())
}

/// 全量替换 installed_skills（工作台勾选提交），返回刷新后的 status 片段。
/// 重复 key（skills=a&skills=b）serde_urlencoded 不支持，用 form_urlencoded 手动收集。
pub async fn set_skills(
    State(state): State<AppState>,
    Path((_token, id)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let skills: Vec<String> = parse(&body)
        .filter(|(k, _)| k.as_ref() == "skills")
        .map(|(_, v)| v.into_owned())
        .collect();
    let Ok(mut proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proj.installed_skills = skills;
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    status_fragment(state, proj)
}

/// status 片段端点（SSE 触发 hx-get 刷新用，Task 14 接入）。
pub async fn status(
    State(state): State<AppState>,
    Path((_token, id)): Path<(String, String)>,
) -> Response {
    let Ok(proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    status_fragment(state, proj)
}

/// apply：调 core run_apply 落地，保存 locked_shas，返回 apply 结果片段。
pub async fn apply(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    let Ok(mut proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let report = match run_apply(&state.paths, &mut proj, false) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = ?e, "apply 失败");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let rendered = ApplyResultTpl {
        token: &token,
        project_id: &id,
        report,
    }
    .render();
    render_str(rendered)
}

fn render_workspace(state: AppState, token: String, proj: Project, fragment: bool) -> Response {
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
    let profiles = skillkit_core::list_profile_names(&state.paths).unwrap_or_default();
    let all_skills: Vec<(SkillMeta, bool)> = reg
        .skills
        .values()
        .map(|m| {
            let installed = proj.installed_skills.iter().any(|s| s == &m.id);
            (m.clone(), installed)
        })
        .collect();
    let rendered = if fragment {
        WorkspaceMainTpl {
            token: &token,
            project: &proj,
            status,
            shared,
            all_skills,
            profiles,
        }
        .render()
    } else {
        WorkspaceTpl {
            token: &token,
            project: &proj,
            status,
            shared,
            all_skills,
            profiles,
        }
        .render()
    };
    render_str(rendered)
}

/// 计算 status 并渲染 fragments/status.html（供 set_skills 返回 + SSE hx-get 刷新）。
fn status_fragment(state: AppState, proj: Project) -> Response {
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
    let rendered = StatusTpl { status }.render();
    render_str(rendered)
}

fn render_list(token: String, projects: Vec<Project>, fragment: bool) -> Response {
    let rendered = if fragment {
        ProjectsMainTpl {
            token: &token,
            projects,
        }
        .render()
    } else {
        ProjectsTpl {
            token: &token,
            projects,
        }
        .render()
    };
    render_str(rendered)
}

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
pub async fn browse(Path(token): Path<String>, Query(q): Query<BrowseQuery>) -> Response {
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
        .map_or_else(|| dir.to_path_buf(), PathBuf::from)
}

fn render_str(rendered: askama::Result<String>) -> Response {
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "渲染 projects 模板失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
