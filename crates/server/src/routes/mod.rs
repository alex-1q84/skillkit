//! 受保护路由装配（/{token}/ 前缀）。各视图 handler 在子模块。
use axum::routing::{delete, get, post};
use axum::Router;

use crate::AppState;

pub mod profiles;
pub mod projects;
pub mod skills;
pub mod sources;
pub mod sse;

pub fn protected() -> Router<AppState> {
    Router::new()
        .route("/{token}", get(crate::home))
        .route("/{token}/", get(crate::home))
        .route("/{token}/sources", get(sources::page).post(sources::add))
        .route("/{token}/sources/preview", get(sources::preview))
        .route("/{token}/sources/{name}", delete(sources::remove))
        .route("/{token}/skills", get(skills::page))
        .route("/{token}/skills/{id}/install", post(skills::install))
        .route("/{token}/skills/{id}", delete(skills::uninstall))
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
        .route("/{token}/projects", get(projects::list))
        .route("/{token}/projects/{id}", get(projects::workspace))
        .route("/{token}/projects/{id}/skills", post(projects::set_skills))
        .route("/{token}/projects/{id}/status", get(projects::status))
        .route("/{token}/projects/{id}/apply", post(projects::apply))
        .route("/{token}/events", get(sse::events))
}
