#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use sha2::Sha256;
use tempfile::TempDir;
use tempmail::api;
use tempmail::config::{AuthConfig, Config, MailConfig, ResendConfig, ServerConfig};
use tempmail::events::EventBus;
use tempmail::state::AppState;
use tokio::sync::broadcast;
use tower::ServiceExt;

pub const API_TOKEN: &str = "test-api-token";
pub const HMAC_SECRET: &str = "test-hmac-secret";

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    pub events: broadcast::Receiver<tempmail::events::Event>,
    _temp_dir: TempDir,
}

#[derive(Clone)]
pub struct TestOptions {
    pub domain: String,
    pub catch_all: bool,
    pub resend_base_url: String,
}

impl Default for TestOptions {
    fn default() -> Self {
        Self {
            domain: "jalencode.com".into(),
            catch_all: true,
            resend_base_url: "http://127.0.0.1:1".into(),
        }
    }
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_options(TestOptions::default()).await
    }

    pub async fn with_options(opts: TestOptions) -> Self {
        let temp = TempDir::new().expect("tempdir");
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
                domain: opts.domain,
                max_message_bytes: 25 * 1024 * 1024,
                catch_all: opts.catch_all,
            },
            resend: ResendConfig {
                api_key: "test-resend-key".into(),
                default_from_name: Some("Test".into()),
                base_url: opts.resend_base_url,
            },
            smtp: None,
        };

        let state = AppState::new(cfg).await.expect("AppState");
        tempmail::MIGRATOR.run(&state.db).await.expect("migrate");

        // Subscribe BEFORE the router takes ownership of state so test sees events.
        let events = state.events.subscribe();
        let router = api::router(state.clone());

        Self {
            router,
            state,
            events,
            _temp_dir: temp,
        }
    }

    pub fn hmac_hex(&self, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(HMAC_SECRET.as_bytes()).unwrap();
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    pub async fn raw(
        &self,
        method: Method,
        path: &str,
        body: Body,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Vec<u8>) {
        let mut req = Request::builder().method(method).uri(path);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let request = req.body(body).expect("build request");
        let response = self.router.clone().oneshot(request).await.expect("oneshot");
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, bytes)
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Vec<u8>) {
        let auth = format!("Bearer {}", API_TOKEN);
        self.raw(Method::GET, path, Body::empty(), &[("authorization", &auth)])
            .await
    }

    pub async fn get_public(&self, path: &str) -> (StatusCode, Vec<u8>) {
        self.raw(Method::GET, path, Body::empty(), &[]).await
    }

    pub async fn get_no_auth(&self, path: &str) -> (StatusCode, Vec<u8>) {
        self.raw(Method::GET, path, Body::empty(), &[]).await
    }

    pub async fn post_json(&self, path: &str, body: &serde_json::Value) -> (StatusCode, Vec<u8>) {
        let auth = format!("Bearer {}", API_TOKEN);
        let bytes = serde_json::to_vec(body).unwrap();
        self.raw(
            Method::POST,
            path,
            Body::from(bytes),
            &[
                ("authorization", &auth),
                ("content-type", "application/json"),
            ],
        )
        .await
    }

    pub async fn post_empty(&self, path: &str) -> (StatusCode, Vec<u8>) {
        let auth = format!("Bearer {}", API_TOKEN);
        self.raw(
            Method::POST,
            path,
            Body::empty(),
            &[("authorization", &auth)],
        )
        .await
    }

    pub async fn delete(&self, path: &str) -> (StatusCode, Vec<u8>) {
        let auth = format!("Bearer {}", API_TOKEN);
        self.raw(
            Method::DELETE,
            path,
            Body::empty(),
            &[("authorization", &auth)],
        )
        .await
    }

    pub async fn ingest_signed(&self, raw_eml: &[u8]) -> (StatusCode, Vec<u8>) {
        let sig = self.hmac_hex(raw_eml);
        self.raw(
            Method::POST,
            "/v1/ingest",
            Body::from(raw_eml.to_vec()),
            &[("x-signature", &sig)],
        )
        .await
    }

    pub async fn ingest_with_sig(&self, raw_eml: &[u8], sig: &str) -> (StatusCode, Vec<u8>) {
        self.raw(
            Method::POST,
            "/v1/ingest",
            Body::from(raw_eml.to_vec()),
            &[("x-signature", sig)],
        )
        .await
    }
}

pub fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> T {
    serde_json::from_slice(body).expect("decode JSON")
}

pub fn sample_eml(to: &str, from: &str, subject: &str, body: &str) -> Vec<u8> {
    format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: Wed, 21 May 2026 12:00:00 +0000\r\nMessage-ID: <test-{n}@example.com>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n",
        n = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    )
    .into_bytes()
}

// Silences unused warnings for the EventBus import.
fn _silence_unused() -> EventBus {
    EventBus::new(1)
}
