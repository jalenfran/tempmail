mod common;

use axum::http::StatusCode;
use common::{TestApp, parse};

#[tokio::test]
async fn no_subject_message_parses_with_null_subject() {
    let app = TestApp::new().await;
    let eml = b"From: s@example.com\r\nTo: alice@jalencode.com\r\nDate: Wed, 21 May 2026 12:00:00 +0000\r\n\r\nbody\r\n";
    let (status, b) = app.ingest_signed(eml).await;
    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&b));
    let v: serde_json::Value = parse(&b);
    let id = v["id"].as_str().unwrap();

    let (_, detail) = app.get(&format!("/v1/messages/{id}")).await;
    let d: serde_json::Value = parse(&detail);
    assert!(d["subject"].is_null());
}

#[tokio::test]
async fn multi_recipient_picks_on_domain() {
    let app = TestApp::new().await;
    let eml = b"From: s@example.com\r\nTo: other@somewhere.com, alice@jalencode.com, bob@otherplace.com\r\nSubject: many\r\nDate: Wed, 21 May 2026 12:00:00 +0000\r\n\r\nbody\r\n";
    let (status, b) = app.ingest_signed(eml).await;
    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&b));
    let v: serde_json::Value = parse(&b);
    let id = v["id"].as_str().unwrap();

    let (_, detail) = app.get(&format!("/v1/messages/{id}")).await;
    let d: serde_json::Value = parse(&detail);
    assert_eq!(d["local_part"], "alice");
}

#[tokio::test]
async fn html_only_message_has_html_no_text() {
    let app = TestApp::new().await;
    let eml = b"From: s@example.com\r\nTo: alice@jalencode.com\r\nSubject: html\r\nDate: Wed, 21 May 2026 12:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Hello <b>world</b></p>\r\n";
    let (status, b) = app.ingest_signed(eml).await;
    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&b));
    let v: serde_json::Value = parse(&b);
    let id = v["id"].as_str().unwrap();

    let (_, detail) = app.get(&format!("/v1/messages/{id}")).await;
    let d: serde_json::Value = parse(&detail);
    assert_eq!(d["has_html"], true);
    assert!(d["html_body"].as_str().unwrap().contains("<b>world</b>"));
}

#[tokio::test]
async fn message_with_attachment_persists_attachment() {
    let app = TestApp::new().await;
    let eml = b"From: s@example.com\r\nTo: alice@jalencode.com\r\nSubject: with attachment\r\nDate: Wed, 21 May 2026 12:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"BOUND\"\r\n\r\n--BOUND\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nLook attached.\r\n--BOUND\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Disposition: attachment; filename=\"note.txt\"\r\n\r\nthe attachment contents\r\n--BOUND--\r\n";
    let (status, b) = app.ingest_signed(eml).await;
    assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&b));
    let v: serde_json::Value = parse(&b);
    let id = v["id"].as_str().unwrap();

    let (_, detail) = app.get(&format!("/v1/messages/{id}")).await;
    let d: serde_json::Value = parse(&detail);
    let atts = d["attachments"].as_array().unwrap();
    assert_eq!(atts.len(), 1);
    assert_eq!(atts[0]["filename"], "note.txt");

    let att_id = atts[0]["id"].as_str().unwrap();
    let (s, raw) = app
        .get(&format!("/v1/messages/{id}/attachments/{att_id}"))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(String::from_utf8_lossy(&raw).trim(), "the attachment contents");
}
