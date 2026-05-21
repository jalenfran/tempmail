mod common;

use axum::http::StatusCode;
use common::{TestApp, parse, sample_eml};
use tempmail::events::Event;

#[tokio::test]
async fn ingest_accepts_well_formed_message() {
    let mut app = TestApp::new().await;
    let eml = sample_eml(
        "hello@jalencode.com",
        "Sender <s@example.com>",
        "Test subject",
        "Body content here.",
    );

    let (status, body) = app.ingest_signed(&eml).await;
    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&body));

    let v: serde_json::Value = parse(&body);
    let id = v["id"].as_str().expect("id");
    assert!(!id.is_empty());

    let evt = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        app.events.recv(),
    )
    .await
    .expect("event timeout")
    .expect("event recv");
    match evt {
        Event::NewMessage { id: ev_id, local_part } => {
            assert_eq!(ev_id, id);
            assert_eq!(local_part, "hello");
        }
        other => panic!("expected NewMessage, got {:?}", other),
    }
}

#[tokio::test]
async fn ingest_rejects_bad_hmac() {
    let app = TestApp::new().await;
    let eml = sample_eml("hello@jalencode.com", "s@example.com", "x", "y");

    let (status, _) = app.ingest_with_sig(&eml, "deadbeef").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ingest_rejects_missing_hmac() {
    let app = TestApp::new().await;
    let eml = sample_eml("hello@jalencode.com", "s@example.com", "x", "y");

    let (status, _) = app
        .raw(
            axum::http::Method::POST,
            "/v1/ingest",
            axum::body::Body::from(eml),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ingest_rejects_off_domain_recipient() {
    let app = TestApp::new().await;
    let eml = sample_eml("hello@example.com", "s@example.com", "x", "y");

    let (status, body) = app.ingest_signed(&eml).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "body={}",
        String::from_utf8_lossy(&body)
    );
}

#[tokio::test]
async fn ingest_rejects_unparseable_garbage() {
    let app = TestApp::new().await;
    let garbage = b"this is not a valid email at all";
    let (status, _) = app.ingest_signed(garbage).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ingest_creates_address_on_first_mail() {
    let app = TestApp::new().await;
    let eml = sample_eml("brand-new@jalencode.com", "s@example.com", "Hi", "hi");

    let (status, _) = app.ingest_signed(&eml).await;
    assert_eq!(status, StatusCode::OK);

    let (s, body) = app.get("/v1/addresses").await;
    assert_eq!(s, StatusCode::OK);
    let list: Vec<serde_json::Value> = parse(&body);
    assert!(list.iter().any(|a| a["local_part"] == "brand-new"));
}

#[tokio::test]
async fn ingest_rejects_unknown_address_when_not_catch_all() {
    let mut opts = common::TestOptions::default();
    opts.catch_all = false;
    let app = TestApp::with_options(opts).await;

    let eml = sample_eml("ghost@jalencode.com", "s@example.com", "x", "y");
    let (status, body) = app.ingest_signed(&eml).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "body={}",
        String::from_utf8_lossy(&body)
    );
}
