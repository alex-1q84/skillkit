mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn ping_returns_pong() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(common::body_string(resp).await, "pong");
}

#[tokio::test]
async fn protected_route_rejects_wrong_token() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn protected_route_accepts_right_token() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn static_asset_served_with_content_type() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/static/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "text/css; charset=utf-8"
    );
}

#[tokio::test]
async fn home_renders_layout_with_nav() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("/test-token/sources"));
    assert!(body.contains("/test-token/projects"));
    assert!(body.contains("htmx.min.js"));
}
