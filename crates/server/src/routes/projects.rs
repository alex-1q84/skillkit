//! Projects 视图：列表 + 工作台（installed/shared/status 三栏只读；操作在 Task 12/13）。
use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use skillkit_core::{
    build_status, compute_diff, scan_shared, ApplyDiff, Project, Registry, StatusView,
};
use std::path::Path as StdPath;

use crate::AppState;

#[derive(Template)]
#[template(path = "projects.html")]
pub struct ProjectsTpl<'a> {
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
}

pub async fn list(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    let mut projects = Vec::new();
    if let Ok(ids) = skillkit_core::list_project_ids(&state.paths) {
        for id in ids {
            if let Ok(p) = Project::load(&state.paths, &id) {
                projects.push(p);
            }
        }
    }
    render_list(token, projects)
}

pub async fn workspace(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    let Ok(proj) = Project::load(&state.paths, &id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
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
    let rendered = WorkspaceTpl {
        token: &token,
        project: &proj,
        status,
        shared,
    }
    .render();
    render_str(rendered)
}

fn render_list(token: String, projects: Vec<Project>) -> Response {
    let rendered = ProjectsTpl {
        token: &token,
        projects,
    }
    .render();
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
