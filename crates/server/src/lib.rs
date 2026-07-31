//! skillkit-server：Axum web server，调 skillkit-core，供 cli 的 serve 子命令调用。

use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rust_embed::RustEmbed;
use skillkit_core::Paths;

/// 共享状态：注入的路径根 + 随机鉴权 token。
#[derive(Clone)]
pub struct AppState {
    pub paths: Paths,
    pub token: String,
}

/// 嵌入的静态资源（htmx / sortable / app.css）。
#[derive(RustEmbed)]
#[folder = "static/"]
struct Asset;

/// 装配 router（测试用 oneshot 打它，serve 用它起真实 server）。
pub fn app(state: AppState) -> Router {
    // Axum 0.8 路由参数用 {token}（非 :token）。所有业务挂在 /{token}/ 下，layer 校验 token。
    let protected = Router::new()
        .route("/{token}", get(home_placeholder))
        .layer(from_fn_with_state(state.clone(), require_token));

    Router::new()
        .route("/ping", get(ping))
        .route("/static/{file}", get(static_handler))
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

/// 静态资源（htmx/sortable/css），公开访问不校验 token（localhost 无泄露风险）。
async fn static_handler(Path(name): Path<String>) -> Response {
    match Asset::get(&name) {
        Some(file) => (
            [(header::CONTENT_TYPE, content_type(&name))],
            file.data.into_owned(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_type(name: &str) -> &'static str {
    if name.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if name.ends_with(".css") {
        "text/css; charset=utf-8"
    } else {
        "application/octet-stream"
    }
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
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(req).await
}

/// 启动 web server：绑 127.0.0.1、生成随机 token、打印带 token 的 URL。
pub async fn serve(port: u16) -> anyhow::Result<()> {
    let paths = Paths::production();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let state = AppState { paths, token: token.clone() };
    let app = app(state);
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("skillkit serve → http://{addr}/{token}/");
    axum::serve(listener, app).await?;
    Ok(())
}

/// 同步入口（供 cli 直接调用，内部建 runtime）。
pub fn run(port: u16) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(serve(port))
}
