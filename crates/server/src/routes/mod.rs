//! 受保护路由装配（/{token}/ 前缀）。各视图 handler 在子模块。
use axum::{routing::get, Router};

use crate::AppState;

pub mod skills;
pub mod sources;

pub fn protected() -> Router<AppState> {
    Router::new()
        .route("/{token}", get(crate::home))
        .route("/{token}/sources", get(sources::page))
        .route("/{token}/skills", get(skills::page))
}
