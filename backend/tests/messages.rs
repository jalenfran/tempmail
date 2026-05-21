mod common;

use axum::http::StatusCode;
use common::{TestApp, parse, sample_eml};
use serde_json::json;

async fn ingest_one(app: &TestApp, to: &str, from: &str, subject: &str, body: &str) -> String {
    let eml = sample_eml(to, from, subject, body);
    let (status, b) = app.ingest_signed(&eml).await;
    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&b));
    let v: serde_json::Value = parse(&b);
    v["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn list_messages_empty_initially() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/v1/messages").await;
    assert_eq!(status, StatusCode::OK);
    let v: Vec<serde_json::Value> = parse(&body);
    assert!(v.is_empty());
}

#[tokio::test]
async fn list_messages_includes_ingested() {
    let app = TestApp::new().await;
    ingest_one(&app, "alice@jalencode.com", "x@example.com", "Hello", "hi").await;

    let (status, body) = app.get("/v1/messages").await;
    assert_eq!(status, StatusCode::OK);
    let v: Vec<serde_json::Value> = parse(&body);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0]["local_part"], "alice");
    assert_eq!(v[0]["subject"], "Hello");
    assert_eq!(v[0]["read"], false);
}

#[tokio::test]
async fn list_messages_filtered_by_address() {
    let app = TestApp::new().await;
    ingest_one(&app, "a@jalencode.com", "x@example.com", "A", "1").await;
    ingest_one(&app, "b@jalencode.com", "x@example.com", "B", "2").await;

    let (status, body) = app.get("/v1/messages?address=a").await;
    assert_eq!(status, StatusCode::OK);
    let v: Vec<serde_json::Value> = parse(&body);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0]["local_part"], "a");
}

#[tokio::test]
async fn get_message_returns_detail_with_bodies() {
    let app = TestApp::new().await;
    let id = ingest_one(
        &app,
        "alice@jalencode.com",
        "Pat <pat@example.com>",
        "Detail test",
        "This is the body.",
    )
    .await;

    let (status, body) = app.get(&format!("/v1/messages/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = parse(&body);
    assert_eq!(v["id"], id);
    assert_eq!(v["from_addr"], "pat@example.com");
    assert_eq!(v["from_name"], "Pat");
    assert!(v["text_body"].as_str().unwrap().contains("This is the body."));
    assert!(v["to_addrs"].as_array().unwrap().contains(&json!("alice@jalencode.com")));
}

#[tokio::test]
async fn mark_read_flips_state_and_unread_count() {
    let app = TestApp::new().await;
    let id = ingest_one(&app, "alice@jalencode.com", "x@example.com", "S", "B").await;

    // Before
    let (_, before) = app.get("/v1/addresses").await;
    let addrs: Vec<serde_json::Value> = parse(&before);
    let alice = addrs.iter().find(|a| a["local_part"] == "alice").unwrap();
    assert_eq!(alice["unread_count"], 1);

    // Mark read
    let (status, _) = app.post_empty(&format!("/v1/messages/{id}/read")).await;
    assert_eq!(status, StatusCode::OK);

    // After
    let (_, after) = app.get("/v1/addresses").await;
    let addrs: Vec<serde_json::Value> = parse(&after);
    let alice = addrs.iter().find(|a| a["local_part"] == "alice").unwrap();
    assert_eq!(alice["unread_count"], 0);

    let (_, msg_body) = app.get(&format!("/v1/messages/{id}")).await;
    let m: serde_json::Value = parse(&msg_body);
    assert_eq!(m["read"], true);
}

#[tokio::test]
async fn delete_message_removes_it() {
    let app = TestApp::new().await;
    let id = ingest_one(&app, "alice@jalencode.com", "x@example.com", "S", "B").await;

    let (status, _) = app.delete(&format!("/v1/messages/{id}")).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app.get(&format!("/v1/messages/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn raw_eml_endpoint_returns_rfc822() {
    let app = TestApp::new().await;
    let id = ingest_one(&app, "alice@jalencode.com", "x@example.com", "Raw", "Raw body").await;

    let (status, body) = app.get(&format!("/v1/messages/{id}/raw")).await;
    assert_eq!(status, StatusCode::OK);
    let s = String::from_utf8_lossy(&body);
    assert!(s.contains("Subject: Raw"));
    assert!(s.contains("Raw body"));
}
