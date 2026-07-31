//! Projects 视图：列表 + 工作台（声明编辑 + apply 闭环；shared 只读）。
use askama::Template;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use form_urlencoded::parse;
use skillkit_core::{
    build_status, compute_diff, run_apply, scan_shared, ApplyDiff, ApplyReport, Project, Registry,
    SkillMeta, StatusView,
};
use std::path::Path as StdPath;

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
        }
        .render()
    } else {
        WorkspaceTpl {
            token: &token,
            project: &proj,
            status,
            shared,
            all_skills,
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

fn render_str(rendered: askama::Result<String>) -> Response {
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "渲染 projects 模板失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
