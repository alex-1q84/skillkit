//! 受保护路由装配（/{token}/ 前缀）。各视图 handler 在子模块。
use axum::routing::{delete, get, post};
use axum::Router;

use crate::AppState;

pub mod profiles;
pub mod projects;
pub mod skills;
pub mod sources;

pub fn protected() -> Router<AppState> {
    Router::new()
        .route("/{token}", get(crate::home))
        .route("/{token}/sources", get(sources::page))
        .route("/{token}/skills", get(skills::page))
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
}
