mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn protected_route_requires_bearer() {
    let app = TestApp::new().await;
    let (status, _) = app.get_no_auth("/v1/addresses").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_rejects_wrong_token() {
    let app = TestApp::new().await;
    let (status, _) = app
        .raw(
            axum::http::Method::GET,
            "/v1/addresses",
            axum::body::Body::empty(),
            &[("authorization", "Bearer not-the-real-token")],
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_accepts_correct_token() {
    let app = TestApp::new().await;
    let (status, _) = app.get("/v1/addresses").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn public_config_does_not_require_auth() {
    let app = TestApp::new().await;
    let (status, body) = app.get_public("/v1/config").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["domain"], "jalencode.com");
}

#[tokio::test]
async fn missing_authorization_header_format_rejected() {
    let app = TestApp::new().await;
    let (status, _) = app
        .raw(
            axum::http::Method::GET,
            "/v1/addresses",
            axum::body::Body::empty(),
            &[("authorization", "totallywrong")],
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
