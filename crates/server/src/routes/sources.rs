//! Sources 视图：展示安装源注册表 + 增删。Source 极简 {name, package}。
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use serde::Deserialize;
use skillkit_core::source::Source;
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
    /// 源名称（覆盖自动推导）；None/空 → 后端从 package 推导
    #[serde(default)]
    name: Option<String>,
    package: Option<String>,
}

pub async fn add(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<SourceForm>,
) -> Response {
    let Some(package) = f.package.filter(|s| !s.trim().is_empty()) else {
        // 引导行动：告诉用户缺什么、怎么补
        return (
            StatusCode::BAD_REQUEST,
            "需提供 package（git url / 本地路径 / owner/repo）",
        )
            .into_response();
    };
    let Some(name) = f
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| skillkit_core::derive_source_name(&package))
    else {
        return (
            StatusCode::BAD_REQUEST,
            "无法从 package 推导源名称（可用 name 字段覆盖）",
        )
            .into_response();
    };
    let mut store = match SourcesStore::load(&state.paths) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = ?e, "加载 sources 失败");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    // 撞名是用户可预期的错误（400 + 引导），save 失败才是内部错误（500）
    if store.get(&name).is_ok() {
        return (StatusCode::BAD_REQUEST, format!("该名称已被源 {name} 占用")).into_response();
    }
    if store
        .add(Source {
            name,
            package: Some(package),
        })
        .is_err()
        || store.save(&state.paths).is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_sources(state, token)
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

/// 实时推导预览：package 输入时由 htmx 调此端点，服务端用 derive_source_name 推导，
/// 返回预填好 value 的 name input 片段（前端零规则副本，业务逻辑只在 core）。
#[derive(askama::Template)]
#[template(path = "fragments/source_name_input.html")]
struct SourceNameInputTpl {
    value: String,
}

#[derive(Deserialize)]
pub struct PreviewQuery {
    package: Option<String>,
}

pub async fn preview(Query(q): Query<PreviewQuery>) -> Response {
    let value = q
        .package
        .as_deref()
        .and_then(skillkit_core::derive_source_name)
        .unwrap_or_default();
    let rendered = SourceNameInputTpl { value }.render();
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "渲染 source name 预览片段失败");
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
