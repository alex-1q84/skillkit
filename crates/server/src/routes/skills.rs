//! Skills 视图：registry 总览（只读；install/uninstall 在 Task 11）。
use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use skillkit_core::{registry::SkillMeta, Registry};

use crate::AppState;

#[derive(Template)]
#[template(path = "skills.html")]
pub struct SkillsTpl<'a> {
    pub token: &'a str,
    pub skills: Vec<SkillMeta>,
}

pub async fn page(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    match Registry::load(&state.paths) {
        Ok(reg) => {
            let rendered = SkillsTpl {
                token: &token,
                skills: reg.skills.values().cloned().collect(),
            }
            .render();
            match rendered {
                Ok(html) => Html(html).into_response(),
                Err(e) => {
                    tracing::error!(error = ?e, "渲染 skills 失败");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 registry 失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
