//! Profiles 视图：列表 + create + add/remove skill + SortableJS 拖拽排序。
use askama::Template;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use form_urlencoded::parse;
use serde::Deserialize;
use skillkit_core::Profile;

use crate::AppState;

#[derive(Template)]
#[template(path = "profiles.html")]
pub struct ProfilesTpl<'a> {
    pub token: &'a str,
    pub profiles: Vec<Profile>,
}

#[derive(Template)]
#[template(path = "fragments/profile_skills.html")]
pub struct ProfileSkillsTpl<'a> {
    pub token: &'a str,
    pub profile: &'a Profile,
}

pub async fn page(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    render_profiles(state, token)
}

fn render_profiles(state: AppState, token: String) -> Response {
    let mut profiles = Vec::new();
    if let Ok(names) = skillkit_core::list_profile_names(&state.paths) {
        for n in names {
            if let Ok(p) = Profile::load(&state.paths, &n) {
                profiles.push(p);
            }
        }
    }
    let rendered = ProfilesTpl {
        token: &token,
        profiles,
    }
    .render();
    render_str(rendered)
}

#[derive(Deserialize)]
pub struct CreateForm {
    name: String,
}

pub async fn create(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<CreateForm>,
) -> Response {
    let p = Profile {
        name: f.name,
        description: String::new(),
        skills: Vec::new(),
    };
    if p.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_profiles(state, token)
}

#[derive(Deserialize)]
pub struct AddSkillForm {
    id: String,
}

pub async fn add_skill(
    State(state): State<AppState>,
    Path((token, name)): Path<(String, String)>,
    Form(f): Form<AddSkillForm>,
) -> Response {
    match Profile::load(&state.paths, &name) {
        Ok(mut p) => {
            if p.add_skill(&f.id).is_err() || p.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            let rendered = ProfileSkillsTpl {
                token: &token,
                profile: &p,
            }
            .render();
            render_str(rendered)
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn remove_skill(
    State(state): State<AppState>,
    Path((token, name, id)): Path<(String, String, String)>,
) -> Response {
    match Profile::load(&state.paths, &name) {
        Ok(mut p) => {
            if p.remove_skill(&id).is_err() || p.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            let rendered = ProfileSkillsTpl {
                token: &token,
                profile: &p,
            }
            .render();
            render_str(rendered)
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// SortableJS 拖拽排序：body 是重复 key 的 urlencoded（order=a&order=b）。
/// serde_urlencoded 不支持重复 key→Vec，用 form_urlencoded::parse 手动收集顺序。
pub async fn reorder(
    State(state): State<AppState>,
    Path((token, name)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let order: Vec<String> = parse(&body)
        .filter(|(k, _)| k.as_ref() == "order")
        .map(|(_, v)| v.into_owned())
        .collect();
    match Profile::load(&state.paths, &name) {
        Ok(mut p) => {
            p.skills = order;
            if p.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            let rendered = ProfileSkillsTpl {
                token: &token,
                profile: &p,
            }
            .render();
            render_str(rendered)
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn render_str(rendered: askama::Result<String>) -> Response {
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "渲染 profile 模板失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
