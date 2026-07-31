//! Skills 视图：registry 总览 + install/uninstall。
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use serde::Deserialize;
use skillkit_core::{registry::SkillMeta, Registry, Scope, SourcesStore};

use crate::routes::FragmentQuery;
use crate::AppState;

#[derive(Template)]
#[template(path = "skills.html")]
pub struct SkillsTpl<'a> {
    pub token: &'a str,
    /// (meta, id 的 path 编码)——id 形如 source/skill，/ 须编码为 %2F 才能放进单段路径参数。
    pub skills: Vec<(SkillMeta, String)>,
}

/// 纯 main 内容片段（SSE 刷新用），不含 nav。
#[derive(Template)]
#[template(path = "fragments/skills_main.html")]
pub struct SkillsMainTpl<'a> {
    pub token: &'a str,
    pub skills: Vec<(SkillMeta, String)>,
}

pub async fn page(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<FragmentQuery>,
) -> Response {
    render_skills(state, token, q.is_fragment())
}

fn render_skills(state: AppState, token: String, fragment: bool) -> Response {
    match Registry::load(&state.paths) {
        Ok(reg) => {
            let skills: Vec<(SkillMeta, String)> = reg
                .skills
                .values()
                .map(|m| (m.clone(), m.id.replace('/', "%2F")))
                .collect();
            let rendered = if fragment {
                SkillsMainTpl {
                    token: &token,
                    skills,
                }
                .render()
            } else {
                SkillsTpl {
                    token: &token,
                    skills,
                }
                .render()
            };
            render_str(rendered)
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 registry 失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct InstallForm {
    scope: Option<String>,
}

pub async fn install(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
    Form(f): Form<InstallForm>,
) -> Response {
    let Some((source, skill)) = id.split_once('/') else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let scope = if matches!(f.scope.as_deref(), Some("global")) {
        Scope::Global
    } else {
        Scope::Local
    };
    // web install 仅支持固定源（有 package）；registry 源（skills.sh）走 CLI find 选候选
    let store = match SourcesStore::load(&state.paths) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = ?e, "加载 sources 失败");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let src = match store.get(source) {
        Ok(s) => s.clone(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(package) = src.package else {
        tracing::error!(
            "registry 源 {source} 的 install 请用 CLI（skillkit install add {source} {skill}）走 find 选候选"
        );
        return StatusCode::BAD_REQUEST.into_response();
    };
    match skillkit_core::install(&state.paths, source, skill, &package, scope) {
        Ok(_) => render_skills(state, token, false),
        Err(e) => {
            tracing::error!(error = ?e, "install 失败：{id}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn uninstall(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    match skillkit_core::uninstall(&state.paths, &id) {
        Ok(()) => render_skills(state, token, false),
        Err(e) => {
            tracing::error!(error = ?e, "uninstall 失败：{id}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn render_str(rendered: askama::Result<String>) -> Response {
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "渲染 skills 模板失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
