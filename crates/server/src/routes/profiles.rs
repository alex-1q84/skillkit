//! Profiles 视图：列表 + create + add/remove skill + SortableJS 拖拽排序。
use askama::Template;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use form_urlencoded::parse;
use serde::Deserialize;
use skillkit_core::Profile;

use crate::routes::FragmentQuery;
use crate::AppState;

#[derive(Template)]
#[template(path = "profiles.html")]
pub struct ProfilesTpl<'a> {
    pub token: &'a str,
    pub profiles: Vec<Profile>,
}

/// 纯 main 内容片段（SSE 刷新用），不含 nav。
#[derive(Template)]
#[template(path = "fragments/profiles_main.html")]
pub struct ProfilesMainTpl<'a> {
    pub token: &'a str,
    pub profiles: Vec<Profile>,
}

#[derive(Template)]
#[template(path = "fragments/profile_skills.html")]
pub struct ProfileSkillsTpl<'a> {
    pub token: &'a str,
    pub profile: &'a Profile,
}

pub async fn page(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<FragmentQuery>,
) -> Response {
    render_profiles(state, token, q.is_fragment())
}

/// 渲染用过滤：剔除 profile.skills 里的 global 引用（legacy 不显示，原数据不 save）。unknown 保留。
fn filter_global_skills(mut p: Profile, reg: &skillkit_core::Registry) -> Profile {
    p.skills.retain(|id| match reg.get(id) {
        Ok(m) => m.scope != skillkit_core::Scope::Global,
        Err(_) => true, // unknown（不在 registry）当 local 保留
    });
    p
}

fn render_profiles(state: AppState, token: String, fragment: bool) -> Response {
    let reg = skillkit_core::Registry::load(&state.paths).unwrap_or_default();
    let mut profiles = Vec::new();
    if let Ok(names) = skillkit_core::list_profile_names(&state.paths) {
        for n in names {
            if let Ok(p) = Profile::load(&state.paths, &n) {
                profiles.push(filter_global_skills(p, &reg));
            }
        }
    }
    let rendered = if fragment {
        ProfilesMainTpl {
            token: &token,
            profiles,
        }
        .render()
    } else {
        ProfilesTpl {
            token: &token,
            profiles,
        }
        .render()
    };
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
    // 存在性校验：同名不覆盖（防清空已有 profile 的 skills）
    if skillkit_core::Profile::load(&state.paths, &f.name).is_ok() {
        tracing::warn!("profile {} 已存在，create 不覆盖", f.name);
        return render_profiles(state, token, false);
    }
    let p = Profile {
        name: f.name,
        description: String::new(),
        skills: Vec::new(),
    };
    if p.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_profiles(state, token, false)
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
            let reg = skillkit_core::Registry::load(&state.paths).unwrap_or_default();
            if p.add_skill(&f.id, &reg).is_err() || p.save(&state.paths).is_err() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use skillkit_core::{Registry, Scope, SkillMeta};

    fn reg(id: &str, scope: Scope) -> Registry {
        let mut r = Registry::default();
        r.upsert(SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: None,
            computed_hash: Some("a".into()),
            installed_at: "t".into(),
            canonical_path: format!(
                "~/.skillkit/.agents/skills/{}",
                id.rsplit('/').next().unwrap()
            ),
        });
        r
    }

    #[test]
    fn filter_global_skills_drops_global_keeps_local_and_unknown() {
        let g = reg("dc/g", Scope::Global);
        let l = reg("dc/l", Scope::Local);
        let mut reg_all = Registry::default();
        reg_all.upsert(g.get("dc/g").cloned().unwrap());
        reg_all.upsert(l.get("dc/l").cloned().unwrap());
        let p = skillkit_core::Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec!["dc/g".into(), "dc/l".into(), "dc/unknown".into()],
        };
        let filtered = filter_global_skills(p, &reg_all);
        assert_eq!(
            filtered.skills,
            vec!["dc/l".to_string(), "dc/unknown".to_string()],
            "global 删，local + unknown 保留"
        );
    }
}
