mod common;

use common::{API_TOKEN, HMAC_SECRET, sample_eml};
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::Duration;
use tempfile::TempDir;
use tempmail::api;
use tempmail::config::{AuthConfig, Config, MailConfig, ResendConfig, ServerConfig};
use tempmail::state::AppState;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// A real axum server bound to a random localhost port for end-to-end testing.
struct LiveServer {
    base_url: String,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
    _temp: TempDir,
}

impl LiveServer {
    async fn start() -> Self {
        let temp = TempDir::new().unwrap();
        let cfg = Config {
            server: ServerConfig {
                listen: "127.0.0.1:0".into(),
                data_dir: temp.path().to_string_lossy().into(),
                public_base_url: "http://test".into(),
            },
            auth: AuthConfig {
                api_token: API_TOKEN.into(),
                ingest_hmac_secret: HMAC_SECRET.into(),
            },
            mail: MailConfig {
                domain: "jalencode.com".into(),
                max_message_bytes: 25 * 1024 * 1024,
                catch_all: true,
            },
            resend: ResendConfig {
                api_key: "test".into(),
                default_from_name: None,
                base_url: "http://127.0.0.1:1".into(),
            },
            smtp: None,
        };
        let state = AppState::new(cfg).await.unwrap();
        tempmail::MIGRATOR.run(&state.db).await.unwrap();
        let app = api::router(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");

        let (tx, rx) = oneshot::channel::<()>();
        let join = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });

        Self {
            base_url,
            shutdown: Some(tx),
            join: Some(join),
            _temp: temp,
        }
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

fn hmac_hex(body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(HMAC_SECRET.as_bytes()).unwrap();
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Reads SSE frames from a byte stream until a `data:` line is found, then
/// returns the parsed JSON payload.
async fn next_sse_payload(
    stream: &mut (impl futures_util::Stream<Item = reqwest::Result<bytes::Bytes>> + Unpin),
    timeout: Duration,
) -> serde_json::Value {
    let mut buffer = String::new();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let chunk = tokio::time::timeout(remaining, stream.next())
            .await
            .expect("SSE timeout")
            .expect("stream ended early")
            .expect("byte chunk");
        buffer.push_str(std::str::from_utf8(&chunk).expect("utf-8"));

        while let Some(nl) = buffer.find('\n') {
            let line: String = buffer.drain(..=nl).collect();
            let line = line.trim_end_matches('\n').trim_end_matches('\r');
            if let Some(payload) = line.strip_prefix("data: ") {
                return serde_json::from_str(payload).expect("json");
            }
        }
    }
}

#[tokio::test]
async fn sse_delivers_new_message_event() {
    let server = LiveServer::start().await;
    let client = reqwest::Client::new();

    // Open the SSE stream first so we're subscribed before the event fires.
    let resp = client
        .get(format!("{}/v1/events", server.base_url))
        .bearer_auth(API_TOKEN)
        .header("accept", "text/event-stream")
        .send()
        .await
        .expect("connect SSE");
    assert!(resp.status().is_success());
    let mut stream = resp.bytes_stream();

    // Small delay to ensure the handler's broadcaster subscription is in place.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Ingest a message.
    let eml = sample_eml("alice@jalencode.com", "s@example.com", "Hello", "Body");
    let sig = hmac_hex(&eml);
    let ingest = client
        .post(format!("{}/v1/ingest", server.base_url))
        .header("x-signature", sig)
        .body(eml)
        .send()
        .await
        .expect("ingest");
    assert!(ingest.status().is_success());
    let ingest_body: serde_json::Value = ingest.json().await.unwrap();
    let expected_id = ingest_body["id"].as_str().unwrap().to_string();

    // Receive the SSE event.
    let payload = next_sse_payload(&mut stream, Duration::from_secs(2)).await;
    assert_eq!(payload["type"], "new_message");
    assert_eq!(payload["id"], expected_id);
    assert_eq!(payload["local_part"], "alice");
}

#[tokio::test]
async fn sse_delivers_address_added_event() {
    let server = LiveServer::start().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/v1/events", server.base_url))
        .bearer_auth(API_TOKEN)
        .header("accept", "text/event-stream")
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let mut stream = resp.bytes_stream();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let res = client
        .post(format!("{}/v1/addresses", server.base_url))
        .bearer_auth(API_TOKEN)
        .json(&serde_json::json!({ "local_part": "via-sse", "pinned": true }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    let payload = next_sse_payload(&mut stream, Duration::from_secs(2)).await;
    assert_eq!(payload["type"], "address_added");
    assert_eq!(payload["local_part"], "via-sse");
}
