mod common;

use axum::http::StatusCode;
use common::{TestApp, TestOptions, parse, sample_eml};
use serde_json::json;

#[tokio::test]
async fn list_addresses_empty_initially() {
    let app = TestApp::new().await;
    let (status, body) = app.get("/v1/addresses").await;
    assert_eq!(status, StatusCode::OK);
    let v: Vec<serde_json::Value> = parse(&body);
    assert!(v.is_empty());
}

#[tokio::test]
async fn create_address_persists_and_pins() {
    let app = TestApp::new().await;
    let (status, body) = app
        .post_json(
            "/v1/addresses",
            &json!({
                "local_part": "rivers-otter-2025",
                "label": "for newsletters",
                "pinned": true,
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&body));
    let v: serde_json::Value = parse(&body);
    assert_eq!(v["local_part"], "rivers-otter-2025");
    assert_eq!(v["label"], "for newsletters");
    assert_eq!(v["pinned"], true);

    let (_, list_body) = app.get("/v1/addresses").await;
    let list: Vec<serde_json::Value> = parse(&list_body);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["local_part"], "rivers-otter-2025");
    assert_eq!(list[0]["pinned"], true);
}

#[tokio::test]
async fn create_address_lowercases_local_part() {
    let app = TestApp::new().await;
    let (status, body) = app
        .post_json(
            "/v1/addresses",
            &json!({ "local_part": "MIXEDcase", "pinned": false }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&body));
    let v: serde_json::Value = parse(&body);
    assert_eq!(v["local_part"], "mixedcase");
}

#[tokio::test]
async fn create_address_rejects_at_sign() {
    let app = TestApp::new().await;
    let (status, _) = app
        .post_json(
            "/v1/addresses",
            &json!({ "local_part": "foo@bar", "pinned": false }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_address_works() {
    let app = TestApp::new().await;
    app.post_json("/v1/addresses", &json!({ "local_part": "kill-me" }))
        .await;
    let (status, _) = app.delete("/v1/addresses/kill-me").await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = app.get("/v1/addresses").await;
    let list: Vec<serde_json::Value> = parse(&body);
    assert!(list.iter().all(|a| a["local_part"] != "kill-me"));
}

#[tokio::test]
async fn freshly_created_address_has_zero_unread() {
    let app = TestApp::new().await;
    let (_, body) = app
        .post_json(
            "/v1/addresses",
            &json!({ "local_part": "stone-shore-75", "pinned": true }),
        )
        .await;
    let v: serde_json::Value = parse(&body);
    assert_eq!(v["unread_count"], 0);
    assert_eq!(v["message_count"], 0);

    let (_, list_body) = app.get("/v1/addresses").await;
    let list: Vec<serde_json::Value> = parse(&list_body);
    let addr = list
        .iter()
        .find(|a| a["local_part"] == "stone-shore-75")
        .unwrap();
    assert_eq!(addr["unread_count"], 0);
    assert_eq!(addr["message_count"], 0);
}

#[tokio::test]
async fn upsert_can_toggle_pinned() {
    let app = TestApp::new().await;
    app.post_json(
        "/v1/addresses",
        &json!({ "local_part": "togglepin", "pinned": true }),
    )
    .await;
    let (_, body) = app
        .post_json(
            "/v1/addresses",
            &json!({ "local_part": "togglepin", "pinned": false }),
        )
        .await;
    let v: serde_json::Value = parse(&body);
    assert_eq!(v["pinned"], false);
}

#[tokio::test]
async fn allowlist_mode_accepts_precreated_address() {
    let mut opts = TestOptions::default();
    opts.catch_all = false;
    let app = TestApp::with_options(opts).await;

    // Pre-create
    let (status, _) = app
        .post_json(
            "/v1/addresses",
            &json!({ "local_part": "allowed", "pinned": false }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Mail to it should be accepted
    let eml = sample_eml("allowed@jalencode.com", "x@example.com", "S", "B");
    let (status, body) = app.ingest_signed(&eml).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "body={}",
        String::from_utf8_lossy(&body)
    );
}
