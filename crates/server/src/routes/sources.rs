//! Sources 视图：展示安装源注册表 + 增删。
use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use serde::Deserialize;
use skillkit_core::source::{Source, SourceType};
use skillkit_core::SourcesStore;

use crate::AppState;

#[derive(Template)]
#[template(path = "sources.html")]
pub struct SourcesTpl<'a> {
    pub token: &'a str,
    pub sources: Vec<Source>,
}

pub async fn page(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    render_sources(state, token)
}

fn render_sources(state: AppState, token: String) -> Response {
    match SourcesStore::load(&state.paths) {
        Ok(store) => {
            let rendered = SourcesTpl {
                token: &token,
                sources: store.list().to_vec(),
            }
            .render();
            render_str(rendered)
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 sources 失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct SourceForm {
    name: String,
    source_type: String,
    url: Option<String>,
    path: Option<String>,
    #[serde(rename = "ref")]
    ref_: Option<String>,
    skills_dir: Option<String>,
}

pub async fn add(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<SourceForm>,
) -> Response {
    let source_type = match f.source_type.as_str() {
        "git" => SourceType::Git,
        "local" => SourceType::Local,
        _ => SourceType::SkillsSh,
    };
    let src = Source {
        name: f.name,
        source_type,
        url: f.url,
        path: f.path,
        ref_: f.ref_,
        skills_dir: f.skills_dir,
    };
    match SourcesStore::load(&state.paths) {
        Ok(mut store) => {
            if store.add(src).is_err() || store.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            render_sources(state, token)
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 sources 失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn remove(
    State(state): State<AppState>,
    Path((token, name)): Path<(String, String)>,
) -> Response {
    match SourcesStore::load(&state.paths) {
        Ok(mut store) => {
            if store.remove(&name).is_err() || store.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            render_sources(state, token)
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 sources 失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn render_str(rendered: askama::Result<String>) -> Response {
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "渲染 sources 模板失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
