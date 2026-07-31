use axum::body::Body;
use axum::response::Response;
use http_body_util::BodyExt;
use skillkit_core::Paths;
use skillkit_server::AppState;
use std::path::PathBuf;

/// 固定 token 的测试 AppState（home 指向 fake 路径；写文件的测试自建 tempdir state）。
pub fn test_state() -> AppState {
    AppState {
        paths: Paths::new(PathBuf::from("/tmp/skillkit-fakehome")),
        token: "test-token".to_string(),
    }
}

/// 同名 token 拼接，便于视图测试构造 uri（后续视图 task 用）。
#[allow(dead_code)]
pub fn uri(path: &str) -> String {
    format!("/test-token/{path}")
}

pub async fn body_string(resp: Response<Body>) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}
