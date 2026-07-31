//! skillkit-server：Axum web server，调 skillkit-core，供 cli 的 serve 子命令调用。

use axum::{
    extract::{Request, State},
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use skillkit_core::Paths;

/// 共享状态：注入的路径根 + 随机鉴权 token。
#[derive(Clone)]
pub struct AppState {
    pub paths: Paths,
    pub token: String,
}

/// 装配 router（测试用 oneshot 打它，serve 用它起真实 server）。
pub fn app(state: AppState) -> Router {
    // Axum 0.8 路由参数用 {token}（非 :token）。所有业务挂在 /{token}/ 下，layer 校验 token。
    let protected = Router::new()
        .route("/{token}", get(home_placeholder))
        .layer(from_fn_with_state(state.clone(), require_token));

    Router::new()
        .route("/ping", get(ping))
        .merge(protected)
        .with_state(state)
}

/// 健康检查（不校验 token）。
async fn ping() -> &'static str {
    "pong"
}

/// home 占位：Task 5 换成渲染 layout。这里先返回 200 让 token 测试成立。
async fn home_placeholder() -> &'static str {
    "skillkit"
}

/// 校验 path 首段 == 预期 token，否则 404（不泄露路由存在性）。
async fn require_token(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let token = req
        .uri()
        .path()
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("");
    if token != state.token {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }
    next.run(req).await
}

/// 启动 web server（Task 4 实现）。
pub fn run(_port: u16) -> anyhow::Result<()> {
    Ok(())
}
