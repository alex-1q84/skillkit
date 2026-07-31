//! skillkit-server：Axum web server，调 skillkit-core，供 cli 的 serve 子命令调用。

use axum::{routing::get, Router};
use skillkit_core::Paths;

/// 共享状态：注入的路径根 + 随机鉴权 token。
#[derive(Clone)]
pub struct AppState {
    pub paths: Paths,
    pub token: String,
}

/// 装配 router（测试用 oneshot 打它，serve 用它起真实 server）。
pub fn app(state: AppState) -> Router {
    Router::new().route("/ping", get(ping)).with_state(state)
}

/// 健康检查（不校验 token）。
async fn ping() -> &'static str {
    "pong"
}

/// 启动 web server（Task 4 实现）。
pub fn run(_port: u16) -> anyhow::Result<()> {
    Ok(())
}
