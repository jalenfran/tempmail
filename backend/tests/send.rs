mod common;

use axum::http::StatusCode;
use common::{TestApp, TestOptions, parse};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn send_calls_resend_and_persists() {
    let resend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "id": "resend-mock-id-123" })),
        )
        .mount(&resend)
        .await;

    let mut opts = TestOptions::default();
    opts.resend_base_url = resend.uri();
    let app = TestApp::with_options(opts).await;

    let (status, body) = app
        .post_json(
            "/v1/send",
            &json!({
                "from_local_part": "hello",
                "to": ["recipient@example.com"],
                "subject": "Hi there",
                "text": "test body",
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&body));
    let v: serde_json::Value = parse(&body);
    assert_eq!(v["resend_id"], "resend-mock-id-123");
    assert!(v["id"].as_str().is_some());

    // The Resend mock should have received exactly one POST.
    let received = resend.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let payload: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(payload["from"], "Test <hello@jalencode.com>");
    assert_eq!(payload["to"][0], "recipient@example.com");
    assert_eq!(payload["subject"], "Hi there");
    assert_eq!(payload["text"], "test body");
}

#[tokio::test]
async fn send_rejects_when_resend_fails() {
    let resend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Resend exploded"))
        .mount(&resend)
        .await;

    let mut opts = TestOptions::default();
    opts.resend_base_url = resend.uri();
    let app = TestApp::with_options(opts).await;

    let (status, _) = app
        .post_json(
            "/v1/send",
            &json!({
                "from_local_part": "hello",
                "to": ["r@example.com"],
                "subject": "boom",
                "text": "x",
            }),
        )
        .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn send_validates_empty_recipients() {
    let app = TestApp::new().await;
    let (status, _) = app
        .post_json(
            "/v1/send",
            &json!({
                "from_local_part": "hello",
                "to": [],
                "subject": "x",
                "text": "y",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn send_requires_text_or_html() {
    let app = TestApp::new().await;
    let (status, _) = app
        .post_json(
            "/v1/send",
            &json!({
                "from_local_part": "hello",
                "to": ["r@example.com"],
                "subject": "x",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
