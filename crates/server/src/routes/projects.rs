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
    detect_agents, run_apply, scan_shared, ApplyReport, Project, Registry, Scope, StatusView,
};
use std::path::{Path as StdPath, PathBuf};

use crate::routes::FragmentQuery;
use crate::AppState;

#[derive(Template)]
#[template(path = "projects.html")]
pub struct ProjectsTpl<'a> {
    pub token: &'a str,
    pub rows: Vec<ProjectRow>,
    pub message: Option<&'a str>,
}

/// 纯 main 内容片段（SSE 刷新用），不含 nav。
#[derive(Template)]
#[template(path = "fragments/projects_main.html")]
pub struct ProjectsMainTpl<'a> {
    pub token: &'a str,
    pub rows: Vec<ProjectRow>,
    pub message: Option<&'a str>,
}

/// 列表项展示数据（handler 预计算 local_count，避免模板调 registry）。
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub local_count: usize,
}

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

/// profile 卡片展示数据（handler 预计算，避免模板调方法）。
pub struct ProfileCard {
    pub name: String,
    pub skill_count: usize,
    pub bound: bool,
}

#[derive(Template)]
#[template(path = "fragments/status.html")]
pub struct StatusTpl {
    pub status: StatusView,
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
}

/// 注册新项目：resolve_dir（展开 ~ + canonicalize）→ 查重 → register（默认 agents）→ save。
/// 重复 path（canonical 全路径精确匹配）拒绝，返回列表 + 顶部提示。
pub async fn add(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<ProjectAddForm>,
) -> Response {
    let abs = resolve_dir(Some(&f.path));
    let abs_str = abs.to_string_lossy().into_owned();
    // load 现有 + 查重（canonical 全路径精确匹配）
    let mut projects = skillkit_core::load_all_projects(&state.paths);
    if projects.iter().any(|p| p.path == abs_str) {
        return render_list(
            &state,
            token,
            projects,
            false,
            Some("该项目已注册，不可重复注册"),
        );
    }
    let agents = detect_agents(&abs);
    let proj = Project::register(abs, agents);
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    projects.push(proj);
    render_list(&state, token, projects, false, None)
}

#[derive(Deserialize)]
pub struct ScanForm {
    pub dir: String,
}

/// scan 候选项：path=全路径，registered=是否已注册（按全路径 canonical 精确匹配）。
pub struct ScanCandidate {
    pub path: String,
    pub registered: bool,
}

#[derive(Template)]
#[template(path = "fragments/scan_results.html")]
pub struct ScanResultsTpl<'a> {
    pub token: &'a str,
    pub candidates: Vec<ScanCandidate>,
}

/// 扫描目录发现项目，渲染候选（每条带 toggle 按钮）。已注册按全路径 canonical 精确匹配标记。
pub async fn scan(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<ScanForm>,
) -> Response {
    match skillkit_core::scan_projects(&resolve_dir(Some(&f.dir)), 3) {
        Ok(dirs) => {
            let registered: std::collections::HashSet<String> =
                skillkit_core::load_all_projects(&state.paths)
                    .into_iter()
                    .map(|p| p.path)
                    .collect();
            let candidates = dirs
                .into_iter()
                .map(|p| {
                    let path = p.to_string_lossy().into_owned();
                    let canon = StdPath::new(&path)
                        .canonicalize()
                        .map_or_else(|_| path.clone(), |c| c.to_string_lossy().into_owned());
                    ScanCandidate {
                        registered: registered.contains(&canon),
                        path,
                    }
                })
                .collect::<Vec<_>>();
            let rendered = ScanResultsTpl {
                token: &token,
                candidates,
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
pub struct ToggleForm {
    pub path: String,
}

#[derive(Template)]
#[template(path = "fragments/scan_toggle.html")]
pub struct ToggleTpl<'a> {
    pub token: &'a str,
    pub path: &'a str,
    pub registered: bool,
}

/// scan 候选 toggle：按全路径 canonical 精确匹配，已注册→注销、未注册→注册。
/// 返回新按钮片段（hx-swap=outerHTML 替换 form），浮层保持可连续 toggle。
pub async fn toggle(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<ToggleForm>,
) -> Response {
    let abs = PathBuf::from(&f.path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&f.path));
    let abs_str = abs.to_string_lossy().into_owned();
    let existing = skillkit_core::load_all_projects(&state.paths)
        .into_iter()
        .find(|p| p.path == abs_str);
    let registered = if let Some(proj) = existing {
        if let Err(e) = skillkit_core::Project::remove(&state.paths, &proj.id) {
            tracing::error!(error = ?e, "toggle 注销失败：{}", proj.id);
        }
        false
    } else {
        let agents = detect_agents(&abs);
        let proj = Project::register(abs, agents);
        if let Err(e) = proj.save(&state.paths) {
            tracing::error!(error = ?e, "toggle 注册失败：{}", f.path);
        }
        true
    };
    let rendered = ToggleTpl {
        token: &token,
        path: &f.path,
        registered,
    }
    .render();
    render_str(rendered)
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
    render_workspace(state, token, proj, false, None)
}

/// 重新探测 agents：按项目内配置目录（.claude/.codex/.cursor/.agents）与指令文件
/// （CLAUDE.md/AGENTS.md）精确判定实际使用的 agent；全部未命中回退开源标准 .agents/。
/// 用于旧项目（注册时默认绑了全 agent）一键校正，避免给未使用的 agent 建目录。
pub async fn sync_agents(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    let Ok(mut proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proj.refresh_agents();
    if proj.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_workspace(state, token, proj, false, None)
}

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
    // 绑定前校正 agents：旧项目可能注册时默认绑了全 agent，改为探测结果避免给
    // 未使用的 agent 建目录。
    proj.refresh_agents();
    // load 所选 profiles；任一不存在给可读 err
    let mut profiles = Vec::new();
    for name in &names {
        match skillkit_core::Profile::load(&state.paths, name) {
            Ok(p) => profiles.push(p),
            Err(_) => {
                return Html(format!(
                    r#"<p class="err">profile 不存在，先去 <a href="/{token}/profiles">Profiles 视图</a>创建。</p>"#
                ))
                .into_response();
            }
        }
    }
    let reg = skillkit_core::Registry::load(&state.paths).unwrap_or_default();
    proj.set_profiles(&names, &profiles, &reg);
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

/// 注销项目：删 toml（不碰项目目录），返回完整 Projects 列表页（写操作返回完整页）。
pub async fn remove(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    if skillkit_core::Project::remove(&state.paths, &id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let projects = skillkit_core::load_all_projects(&state.paths);
    render_list(&state, token, projects, false, None)
}

pub async fn list(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<FragmentQuery>,
) -> Response {
    let projects = skillkit_core::load_all_projects(&state.paths);
    render_list(&state, token, projects, q.is_fragment(), None)
}

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

fn render_workspace(
    state: AppState,
    token: String,
    proj: Project,
    fragment: bool,
    report: Option<ApplyReport>,
) -> Response {
    let reg = Registry::load(&state.paths).unwrap_or_default();
    // 管线组装在 core::compute_status；这里只做 GUI 容错降级（空视图防白屏）
    let status = skillkit_core::compute_status(&state.paths, &proj).unwrap_or_default();
    let shared = scan_shared(StdPath::new(&proj.path), &proj.agents);
    let local_skills: Vec<String> = proj
        .installed_skills
        .iter()
        .filter(|id| reg.get(id).is_ok_and(|m| m.scope == Scope::Local))
        .cloned()
        .collect();
    let profiles: Vec<ProfileCard> = skillkit_core::list_profile_names(&state.paths)
        .unwrap_or_default()
        .into_iter()
        .map(|name| {
            let skill_count =
                skillkit_core::Profile::load(&state.paths, &name).map_or(0, |p| p.skills.len());
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

/// 计算 status 并渲染 fragments/status.html（供 set_skills 返回 + SSE hx-get 刷新）。
fn status_fragment(state: AppState, proj: Project) -> Response {
    // 管线组装在 core::compute_status；GUI 容错降级（空视图防白屏）
    let status = skillkit_core::compute_status(&state.paths, &proj).unwrap_or_default();
    let rendered = StatusTpl { status }.render();
    render_str(rendered)
}

fn render_list(
    state: &AppState,
    token: String,
    projects: Vec<Project>,
    fragment: bool,
    message: Option<&str>,
) -> Response {
    let reg = Registry::load(&state.paths).unwrap_or_default();
    let rows: Vec<ProjectRow> = projects
        .iter()
        .map(|p| {
            let local_count = p
                .installed_skills
                .iter()
                .filter(|id| reg.get(id).is_ok_and(|m| m.scope == Scope::Local))
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
            message,
        }
        .render()
    } else {
        ProjectsTpl {
            token: &token,
            rows,
            message,
        }
        .render()
    };
    render_str(rendered)
}

#[derive(Deserialize)]
pub struct BrowseQuery {
    /// 要列的目录（空/无效 → home）。alias "dir"：扫描表单 input name=dir 也走这里。
    #[serde(alias = "dir")]
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
    pub panel: &'a str,
    pub candidates: Vec<Candidate>,
}

/// 路径输入框 Tab 补全：拆「基准目录 + 前缀」。
/// - 尾斜杠或空 → base=path（解析后），prefix=""（列全部子目录）
/// - 否则 → base=parent，prefix=末段（前缀匹配）
///
/// `~` 与空按 home 解析（复用 resolve_dir）。
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
    let base = match resolved.parent() {
        Some(p) => p.to_path_buf(),
        None => resolved,
    };
    (base, prefix)
}

/// Tab 补全：列 base 下前缀匹配的子目录候选，渲染 complete.html。
pub async fn complete(Path(_token): Path<String>, Query(q): Query<CompleteQuery>) -> Response {
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
        panel: &q.panel,
        candidates,
    }
    .render();
    render_str(rendered)
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
