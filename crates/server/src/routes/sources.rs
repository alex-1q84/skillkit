//! Sources 视图：展示安装源注册表（只读；CRUD 片段在 Task 11）。
use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use skillkit_core::{source::Source, SourcesStore};

use crate::AppState;

#[derive(Template)]
#[template(path = "sources.html")]
pub struct SourcesTpl<'a> {
    pub token: &'a str,
    pub sources: Vec<Source>,
}

pub async fn page(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    match SourcesStore::load(&state.paths) {
        Ok(store) => {
            let rendered = SourcesTpl {
                token: &token,
                sources: store.list().to_vec(),
            }
            .render();
            match rendered {
                Ok(html) => Html(html).into_response(),
                Err(e) => {
                    tracing::error!(error = ?e, "渲染 sources 失败");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 sources 失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
