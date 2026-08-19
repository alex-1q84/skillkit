//! 受保护路由装配（/{token}/ 前缀）。各视图 handler 在子模块。
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{delete, get, post};
use axum::Router;
use serde::Deserialize;

use crate::AppState;

pub mod profiles;
pub mod projects;
pub mod skills;
pub mod sources;
pub mod sse;

/// 写操作错误统一响应：422 + Json{"error"}（htmx 收到 4xx 不 swap，layout JS 弹 toast，不刷页）。
pub fn error_response(msg: impl std::fmt::Display) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(std::collections::HashMap::from([(
            "error".to_string(),
            msg.to_string(),
        )])),
    )
        .into_response()
}

/// 页面 GET 的 query：?fragment=1 时返回纯 main 内容（SSE 刷新用），
/// 否则返回完整页（含 nav 的 layout）。保证 SSE 刷新响应不含 nav，防导航重复。
#[derive(Debug, Default, Deserialize)]
pub struct FragmentQuery {
    pub fragment: Option<String>,
}

impl FragmentQuery {
    pub fn is_fragment(&self) -> bool {
        self.fragment.as_deref() == Some("1")
    }
}

/// Skills 页专属 query：fragment（SSE 片段）+ selected（高亮选中）+ profiles/unassigned/scope（过滤）。
/// 不复用 FragmentQuery——后者只有 fragment 字段，serde 默认忽略未知字段，不扩就静默丢参。
/// selected/profiles 用 CSV（?selected=a,b），serde_urlencoded 对 Vec 字段的单值会拒绝，CSV 单/多值都兼容。
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub struct SkillsQuery {
    pub fragment: Option<String>,
    #[serde(default)]
    pub selected: Option<String>,
    #[serde(default)]
    pub profiles: Option<String>,
    /// 按 scope 筛选：global | local。None=全部。
    #[serde(default)]
    pub scope: Option<String>,
    /// 只显未纳入任何 profile 的 skill（=1 生效）。
    #[serde(default)]
    pub unassigned: Option<String>,
}

impl SkillsQuery {
    pub fn is_fragment(&self) -> bool {
        self.fragment.as_deref() == Some("1")
    }
    pub fn selected_list(&self) -> Vec<String> {
        parse_csv(self.selected.as_deref())
    }
    pub fn profile_filter(&self) -> Vec<String> {
        parse_csv(self.profiles.as_deref())
    }
    pub fn scope_filter(&self) -> Option<skillkit_core::Scope> {
        match self.scope.as_deref() {
            Some("global") => Some(skillkit_core::Scope::Global),
            Some("local") => Some(skillkit_core::Scope::Local),
            _ => None,
        }
    }
    pub fn is_unassigned(&self) -> bool {
        self.unassigned.as_deref() == Some("1")
    }
}

fn parse_csv(o: Option<&str>) -> Vec<String> {
    o.map(|s| {
        s.split(',')
            .filter(|x| !x.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

pub fn protected() -> Router<AppState> {
    Router::new()
        .route("/{token}", get(crate::home))
        .route("/{token}/", get(crate::home))
        .route("/{token}/sources", get(sources::page).post(sources::add))
        .route("/{token}/sources/preview", get(sources::preview))
        .route("/{token}/sources/{name}", delete(sources::remove))
        .route("/{token}/skills", get(skills::page))
        .route("/{token}/skills/find", get(skills::find))
        .route(
            "/{token}/skills/install-candidate",
            post(skills::install_candidate),
        )
        .route("/{token}/skills/import", post(skills::import))
        .route(
            "/{token}/skills/install-local",
            get(skills::install_local_form).post(skills::install_local),
        )
        .route(
            "/{token}/skills/install-local/upload",
            post(skills::install_local_upload).layer(axum::extract::DefaultBodyLimit::max(
                skills::MAX_UPLOAD_BYTES,
            )),
        )
        .route("/{token}/skills/upgrade-all", post(skills::upgrade_all))
        .route("/{token}/skills/assign", post(skills::assign))
        .route("/{token}/skills/assign-new", post(skills::assign_new))
        .route("/{token}/skills/unassign", post(skills::unassign))
        .route("/{token}/skills/rescope", post(skills::rescope))
        .route("/{token}/skills/{id}/install", post(skills::install))
        .route("/{token}/skills/{id}", delete(skills::uninstall))
        .route("/{token}/skills/{id}/upgrade", post(skills::upgrade))
        .route(
            "/{token}/skills/{id}/profile/{name}",
            delete(skills::delete_profile),
        )
        .route(
            "/{token}/profiles",
            get(profiles::page).post(profiles::create),
        )
        .route(
            "/{token}/profiles/{name}/skills",
            get(profiles::page).post(profiles::add_skill),
        )
        .route(
            "/{token}/profiles/{name}/skills/{id}",
            delete(profiles::remove_skill),
        )
        .route("/{token}/profiles/{name}/reorder", post(profiles::reorder))
        .route("/{token}/projects", get(projects::list).post(projects::add))
        .route("/{token}/projects/scan", post(projects::scan))
        .route("/{token}/projects/toggle", post(projects::toggle))
        .route("/{token}/projects/browse", get(projects::browse))
        .route("/{token}/projects/complete", get(projects::complete))
        .route(
            "/{token}/projects/{id}",
            get(projects::workspace).delete(projects::remove),
        )
        .route("/{token}/projects/{id}/rebind", post(projects::rebind))
        .route(
            "/{token}/projects/{id}/sync-agents",
            post(projects::sync_agents),
        )
        .route(
            "/{token}/projects/{id}/profiles",
            post(projects::set_profiles),
        )
        .route("/{token}/projects/{id}/status", get(projects::status))
        .route("/{token}/events", get(sse::events))
}
