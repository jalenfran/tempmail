//! Conformance tests for the RFC 5321 SMTP receiver.
//!
//! Each test boots a real TCP listener on 127.0.0.1:0 (so cargo can run them in
//! parallel without port collisions), drives it with a hand-rolled SMTP client
//! over `tokio::net::TcpStream`, and inspects either the reply codes or the
//! persisted message via the HTTP API.

mod common;

use common::API_TOKEN;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tempmail::config::{
    AuthConfig, Config, MailConfig, ResendConfig, ServerConfig, SmtpConfig,
};
use tempmail::smtp;
use tempmail::state::AppState;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

// ─── test harness ───────────────────────────────────────────────────────────

struct LiveSmtp {
    smtp_addr: std::net::SocketAddr,
    http_router: axum::Router,
    _temp: TempDir,
}

async fn start() -> LiveSmtp {
    start_inner(25 * 1024 * 1024, false).await
}

async fn start_with(max_message_bytes: usize) -> LiveSmtp {
    start_inner(max_message_bytes, false).await
}

async fn start_with_tls() -> LiveSmtp {
    start_inner(25 * 1024 * 1024, true).await
}

async fn start_inner(max_message_bytes: usize, with_tls: bool) -> LiveSmtp {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let temp = TempDir::new().unwrap();
    let cfg = Config {
        server: ServerConfig {
            listen: "127.0.0.1:0".into(),
            data_dir: temp.path().to_string_lossy().into(),
            public_base_url: "http://test".into(),
        },
        auth: AuthConfig {
            api_token: API_TOKEN.into(),
            ingest_hmac_secret: "irrelevant".into(),
        },
        mail: MailConfig {
            domain: "jalencode.com".into(),
            max_message_bytes,
            catch_all: true,
        },
        resend: ResendConfig {
            api_key: "test".into(),
            default_from_name: None,
            base_url: "http://127.0.0.1:1".into(),
        },
        smtp: Some(SmtpConfig {
            listen: "127.0.0.1:0".into(),
            hostname: "tempmail-test".into(),
            tls_cert: None,
            tls_key: None,
        }),
    };

    let state = AppState::new(cfg.clone()).await.unwrap();
    tempmail::MIGRATOR.run(&state.db).await.unwrap();

    let acceptor: Option<Arc<TlsAcceptor>> = if with_tls {
        let (cert_pem, key_pem) = generate_self_signed();
        let server_config =
            tempmail::smtp::build_server_config_from_pem(cert_pem.as_bytes(), key_pem.as_bytes())
                .expect("build TLS config");
        Some(Arc::new(TlsAcceptor::from(Arc::new(server_config))))
    } else {
        None
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let smtp_addr = listener.local_addr().unwrap();
    let smtp_state = state.clone();
    let smtp_cfg = cfg.smtp.clone().unwrap();
    let acceptor_for_serve = acceptor.clone();
    tokio::spawn(async move {
        let _ = smtp::serve(listener, smtp_cfg, smtp_state, acceptor_for_serve).await;
    });

    let http_router = tempmail::api::router(state);

    LiveSmtp {
        smtp_addr,
        http_router,
        _temp: temp,
    }
}

fn generate_self_signed() -> (String, String) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("rcgen");
    (cert.cert.pem(), cert.key_pair.serialize_pem())
}

struct Session {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: BufWriter<tokio::net::tcp::OwnedWriteHalf>,
}

impl Session {
    async fn connect(addr: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(addr).await.expect("connect");
        let (r, w) = stream.into_split();
        Self {
            reader: BufReader::new(r),
            writer: BufWriter::new(w),
        }
    }

    async fn send_line(&mut self, s: &str) {
        self.writer.write_all(s.as_bytes()).await.unwrap();
        self.writer.write_all(b"\r\n").await.unwrap();
        self.writer.flush().await.unwrap();
    }

    async fn send_raw(&mut self, s: &str) {
        self.writer.write_all(s.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }

    /// Read one logical SMTP reply (single line, or multi-line ending with "code SP").
    /// Returns (terminating reply code as 3 chars, full last line, all lines collected).
    async fn read_reply(&mut self) -> SmtpReply {
        let mut all = Vec::new();
        loop {
            let mut line = String::new();
            let n = tokio::time::timeout(
                Duration::from_secs(2),
                self.reader.read_line(&mut line),
            )
            .await
            .expect("read reply timed out")
            .expect("read");
            assert!(n > 0, "smtp closed unexpectedly mid-reply");
            let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
            all.push(trimmed.clone());
            if trimmed.len() < 4 {
                continue;
            }
            // SMTP framing: continuation when 4th char is '-', final when ' '.
            let sep = trimmed.as_bytes()[3];
            if sep == b' ' {
                let code = trimmed[..3].to_string();
                return SmtpReply { code, last: trimmed, all };
            }
        }
    }

    async fn expect_code(&mut self, code: &str) -> SmtpReply {
        let r = self.read_reply().await;
        assert_eq!(r.code, code, "reply: {:?}", r.last);
        r
    }
}

struct SmtpReply {
    code: String,
    last: String,
    all: Vec<String>,
}

async fn fetch_messages_for(server: &LiveSmtp, addr: &str) -> Vec<serde_json::Value> {
    use axum::body::Body;
    use axum::http::{Method, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/messages?address={addr}"))
        .header("authorization", format!("Bearer {API_TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = server.http_router.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn fetch_message_detail(server: &LiveSmtp, id: &str) -> serde_json::Value {
    use axum::body::Body;
    use axum::http::{Method, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/messages/{id}"))
        .header("authorization", format!("Bearer {API_TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = server.http_router.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// Minimal valid mail message for use in DATA sections.
fn sample_data(to: &str, from: &str, subject: &str, body: &str) -> String {
    format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: Wed, 21 May 2026 12:00:00 +0000\r\n\r\n{body}\r\n.\r\n",
    )
}

// ─── connection / greeting ──────────────────────────────────────────────────

#[tokio::test]
async fn banner_is_220_with_esmtp_keyword() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    let r = c.read_reply().await;
    assert_eq!(r.code, "220");
    assert!(r.last.contains("ESMTP"), "banner missing ESMTP: {:?}", r.last);
}

#[tokio::test]
async fn quit_from_fresh_session_is_221_and_closes() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("QUIT").await;
    c.expect_code("221").await;
}

#[tokio::test]
async fn quit_with_argument_is_501() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("QUIT abc").await;
    c.expect_code("501").await;
}

// ─── HELO / EHLO ────────────────────────────────────────────────────────────

#[tokio::test]
async fn helo_returns_single_line_250() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("HELO sender.example.com").await;
    let r = c.read_reply().await;
    assert_eq!(r.code, "250");
    assert_eq!(r.all.len(), 1, "HELO must not advertise extensions");
}

#[tokio::test]
async fn ehlo_returns_multiline_with_extensions() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender.example.com").await;
    let r = c.read_reply().await;
    assert_eq!(r.code, "250");
    assert!(r.all.len() >= 2, "EHLO must advertise extensions");
    let joined = r.all.join("\n");
    assert!(joined.contains("SIZE"), "missing SIZE: {joined}");
    assert!(joined.contains("8BITMIME"), "missing 8BITMIME: {joined}");
    assert!(joined.contains("PIPELINING"), "missing PIPELINING: {joined}");
}

#[tokio::test]
async fn ehlo_without_domain_is_501() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO").await;
    c.expect_code("501").await;
}

#[tokio::test]
async fn ehlo_is_case_insensitive() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("ehlo sender").await;
    c.expect_code("250").await;
}

#[tokio::test]
async fn unknown_command_is_500() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("FROBNICATE").await;
    c.expect_code("500").await;
}

// ─── state machine ──────────────────────────────────────────────────────────

#[tokio::test]
async fn mail_before_helo_is_503() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("MAIL FROM:<a@b>").await;
    c.expect_code("503").await;
}

#[tokio::test]
async fn rcpt_before_mail_is_503() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<x@jalencode.com>").await;
    c.expect_code("503").await;
}

#[tokio::test]
async fn data_before_rcpt_is_503() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@b>").await;
    c.expect_code("250").await;
    c.send_line("DATA").await;
    c.expect_code("503").await;
}

#[tokio::test]
async fn data_with_argument_is_501() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@b>").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<x@jalencode.com>").await;
    c.expect_code("250").await;
    c.send_line("DATA bogus").await;
    c.expect_code("501").await;
}

#[tokio::test]
async fn rset_returns_to_greeted() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@b>").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<x@jalencode.com>").await;
    c.expect_code("250").await;
    c.send_line("RSET").await;
    c.expect_code("250").await;
    // After RSET, RCPT TO should fail again (no MAIL FROM).
    c.send_line("RCPT TO:<x@jalencode.com>").await;
    c.expect_code("503").await;
}

// ─── address parsing ────────────────────────────────────────────────────────

#[tokio::test]
async fn null_sender_is_accepted() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO bounce.example.com").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<>").await;
    c.expect_code("250").await;
}

#[tokio::test]
async fn mail_from_with_size_parameter_is_accepted() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@example.com> SIZE=1234").await;
    c.expect_code("250").await;
}

#[tokio::test]
async fn off_domain_recipient_is_550() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@example.com>").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<x@somewhere-else.com>").await;
    c.expect_code("550").await;
}

#[tokio::test]
async fn unbracketed_address_falls_back() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:a@example.com").await;
    c.expect_code("250").await;
}

#[tokio::test]
async fn unterminated_angle_bracket_is_501() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@example.com").await;
    c.expect_code("501").await;
}

// ─── DATA semantics ─────────────────────────────────────────────────────────

#[tokio::test]
async fn full_transaction_stores_message_with_received_header() {
    let server = start().await;
    let mut c = Session::connect(server.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender.example.com").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<alice@example.com>").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<hello@jalencode.com>").await;
    c.expect_code("250").await;
    c.send_line("DATA").await;
    c.expect_code("354").await;
    c.send_raw(&sample_data(
        "hello@jalencode.com",
        "alice@example.com",
        "ok",
        "hi",
    ))
    .await;
    c.expect_code("250").await;

    let list = fetch_messages_for(&server, "hello").await;
    assert_eq!(list.len(), 1);
    let id = list[0]["id"].as_str().unwrap();
    let detail = fetch_message_detail(&server, id).await;

    // Received: header should be present in stored body.
    use axum::body::Body;
    use axum::http::{Method, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/messages/{id}/raw"))
        .header("authorization", format!("Bearer {API_TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = server.http_router.clone().oneshot(req).await.unwrap();
    let raw = resp.into_body().collect().await.unwrap().to_bytes();
    let raw_str = String::from_utf8_lossy(&raw);
    assert!(
        raw_str.starts_with("Received: from sender.example.com"),
        "raw must start with Received: trace, got: {}",
        &raw_str[..raw_str.len().min(200)]
    );
    assert!(raw_str.contains("by tempmail-test with ESMTP"));
    assert_eq!(detail["subject"], "ok");
}

#[tokio::test]
async fn multiple_recipients_all_recorded() {
    let server = start().await;
    let mut c = Session::connect(server.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@example.com>").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<first@jalencode.com>").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<second@jalencode.com>").await;
    c.expect_code("250").await;
    c.send_line("DATA").await;
    c.expect_code("354").await;
    c.send_raw(
        "From: a@example.com\r\nTo: first@jalencode.com, second@jalencode.com\r\nSubject: multi\r\nDate: Wed, 21 May 2026 12:00:00 +0000\r\n\r\nhi\r\n.\r\n",
    )
    .await;
    c.expect_code("250").await;

    // The message is filed under the first on-domain recipient parsed from the
    // To: header (or RCPT TO if not present). We just verify it landed.
    let list = fetch_messages_for(&server, "first").await;
    assert_eq!(list.len(), 1);
    let detail = fetch_message_detail(&server, list[0]["id"].as_str().unwrap()).await;
    let tos = detail["to_addrs"].as_array().unwrap();
    assert!(tos.iter().any(|a| a == "first@jalencode.com"));
    assert!(tos.iter().any(|a| a == "second@jalencode.com"));
}

#[tokio::test]
async fn dot_stuffing_is_decoded() {
    let server = start().await;
    let mut c = Session::connect(server.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@example.com>").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<dots@jalencode.com>").await;
    c.expect_code("250").await;
    c.send_line("DATA").await;
    c.expect_code("354").await;
    // Lines starting with "." on the wire get one "." stripped. ".." → "."
    c.send_raw("From: a@example.com\r\nTo: dots@jalencode.com\r\nSubject: dots\r\nDate: Wed, 21 May 2026 12:00:00 +0000\r\n\r\nline one\r\n..should be one dot\r\n.\r\n").await;
    c.expect_code("250").await;

    let list = fetch_messages_for(&server, "dots").await;
    let detail = fetch_message_detail(&server, list[0]["id"].as_str().unwrap()).await;
    let text = detail["text_body"].as_str().unwrap();
    assert!(text.contains(".should be one dot"));
    assert!(!text.contains("..should"));
}

#[tokio::test]
async fn data_oversize_returns_552_and_recovers() {
    let server = start_with(1024).await;
    let mut c = Session::connect(server.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@example.com>").await;
    c.expect_code("250").await;
    c.send_line("RCPT TO:<huge@jalencode.com>").await;
    c.expect_code("250").await;
    c.send_line("DATA").await;
    c.expect_code("354").await;
    // 2 KB of body > 1 KB cap. Spread across short lines to exercise the
    // accumulating size check, not the per-line check.
    let mut payload =
        "From: a@example.com\r\nTo: huge@jalencode.com\r\nSubject: big\r\n\r\n".to_string();
    for _ in 0..40 {
        payload.push_str(&"x".repeat(60));
        payload.push_str("\r\n");
    }
    payload.push_str(".\r\n");
    c.send_raw(&payload).await;
    c.expect_code("552").await;

    // Session must recover for a new transaction.
    c.send_line("MAIL FROM:<a@example.com>").await;
    c.expect_code("250").await;
}

#[tokio::test]
async fn multiple_transactions_in_one_session() {
    let server = start().await;
    let mut c = Session::connect(server.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;

    for addr in &["one", "two", "three"] {
        c.send_line("MAIL FROM:<a@example.com>").await;
        c.expect_code("250").await;
        c.send_line(&format!("RCPT TO:<{addr}@jalencode.com>")).await;
        c.expect_code("250").await;
        c.send_line("DATA").await;
        c.expect_code("354").await;
        c.send_raw(&sample_data(
            &format!("{addr}@jalencode.com"),
            "a@example.com",
            "subject",
            "body",
        ))
        .await;
        c.expect_code("250").await;
    }

    for addr in &["one", "two", "three"] {
        let list = fetch_messages_for(&server, addr).await;
        assert_eq!(list.len(), 1, "{addr} should have one message");
    }
}

// ─── pipelining ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn pipelined_commands_processed_in_order() {
    let server = start().await;
    let mut c = Session::connect(server.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;

    // Per RFC 2920, MAIL/RCPT/RCPT/DATA may be sent without waiting for replies.
    c.send_raw(
        "MAIL FROM:<a@example.com>\r\n\
         RCPT TO:<a@jalencode.com>\r\n\
         RCPT TO:<b@jalencode.com>\r\n\
         DATA\r\n",
    )
    .await;
    c.expect_code("250").await; // MAIL
    c.expect_code("250").await; // RCPT a
    c.expect_code("250").await; // RCPT b
    c.expect_code("354").await; // DATA

    c.send_raw(&sample_data(
        "a@jalencode.com",
        "a@example.com",
        "pipelined",
        "body",
    ))
    .await;
    c.expect_code("250").await;
}

// ─── line length limits ────────────────────────────────────────────────────

#[tokio::test]
async fn oversize_command_line_is_500_and_recovers() {
    let server = start().await;
    let mut c = Session::connect(server.smtp_addr).await;
    c.expect_code("220").await;
    // Build a line longer than 512 octets.
    let mut long = String::from("EHLO ");
    long.push_str(&"a".repeat(600));
    c.send_line(&long).await;
    c.expect_code("500").await;
    // Recover.
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
}

// ─── miscellaneous ──────────────────────────────────────────────────────────

#[tokio::test]
async fn noop_replies_250() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("NOOP").await;
    c.expect_code("250").await;
}

#[tokio::test]
async fn vrfy_returns_252() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("VRFY anyone").await;
    c.expect_code("252").await;
}

#[tokio::test]
async fn expn_returns_252() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EXPN list").await;
    c.expect_code("252").await;
}

#[tokio::test]
async fn help_returns_214() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("HELP").await;
    c.expect_code("214").await;
}

#[tokio::test]
async fn starttls_returns_502_when_not_configured() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    let r = c.read_reply().await;
    assert_eq!(r.code, "250");
    let joined = r.all.join("\n");
    assert!(
        !joined.contains("STARTTLS"),
        "STARTTLS must not be advertised when not configured: {joined}"
    );
    c.send_line("STARTTLS").await;
    c.expect_code("502").await;
}

#[tokio::test]
async fn auth_not_implemented_returns_502() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("AUTH LOGIN").await;
    c.expect_code("502").await;
}

// ─── STARTTLS ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn starttls_advertised_when_cert_configured() {
    let s = start_with_tls().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    let r = c.read_reply().await;
    assert_eq!(r.code, "250");
    let joined = r.all.join("\n");
    assert!(
        joined.contains("STARTTLS"),
        "STARTTLS must be advertised: {joined}"
    );
}

#[tokio::test]
async fn starttls_handshake_then_re_ehlo_and_send_over_tls() {
    let server = start_with_tls().await;
    let mut c = Session::connect(server.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.expect_code("250").await;
    c.send_line("STARTTLS").await;
    c.expect_code("220").await;

    // Now upgrade the client side to TLS.
    let tls_stream = upgrade_client(c).await;
    let (read, write) = tokio::io::split(tls_stream);
    let mut t = TlsSession {
        reader: BufReader::new(read),
        writer: BufWriter::new(write),
    };

    // RFC 3207: after STARTTLS, client must re-issue EHLO. Server must not
    // advertise STARTTLS again on the TLS-protected channel.
    t.send_line("EHLO sender.example.com").await;
    let r = t.read_reply().await;
    assert_eq!(r.code, "250");
    let joined = r.all.join("\n");
    assert!(
        !joined.contains("STARTTLS"),
        "STARTTLS must not appear after upgrade: {joined}"
    );

    // Full transaction over TLS.
    t.send_line("MAIL FROM:<a@example.com>").await;
    t.expect_code("250").await;
    t.send_line("RCPT TO:<tls@jalencode.com>").await;
    t.expect_code("250").await;
    t.send_line("DATA").await;
    t.expect_code("354").await;
    t.send_raw(&sample_data(
        "tls@jalencode.com",
        "a@example.com",
        "over-tls",
        "encrypted hello",
    ))
    .await;
    t.expect_code("250").await;

    // Verify the message landed, and the Received: header reflects ESMTPS.
    let list = fetch_messages_for(&server, "tls").await;
    assert_eq!(list.len(), 1);
    let id = list[0]["id"].as_str().unwrap();

    use axum::body::Body;
    use axum::http::{Method, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/messages/{id}/raw"))
        .header("authorization", format!("Bearer {API_TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = server.http_router.clone().oneshot(req).await.unwrap();
    let raw = resp.into_body().collect().await.unwrap().to_bytes();
    let raw_str = String::from_utf8_lossy(&raw);
    assert!(
        raw_str.contains("with ESMTPS"),
        "Received header must indicate ESMTPS over TLS: {raw_str}"
    );
}

#[tokio::test]
async fn starttls_with_argument_is_501() {
    let s = start_with_tls().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO sender").await;
    c.read_reply().await;
    c.send_line("STARTTLS extra").await;
    c.expect_code("501").await;
}

// Helpers for TLS-side tests.

async fn upgrade_client(s: Session) -> tokio_rustls::client::TlsStream<TcpStream> {
    // Recombine the split halves back into a TcpStream.
    let Session { reader, writer } = s;
    let read = reader.into_inner();
    let write = writer.into_inner();
    let tcp = read.reunite(write).expect("reunite tcp halves");

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertVerify))
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    connector
        .connect(server_name, tcp)
        .await
        .expect("TLS handshake")
}

struct TlsSession {
    reader: BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
    writer: BufWriter<tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
}

impl TlsSession {
    async fn send_line(&mut self, s: &str) {
        self.writer.write_all(s.as_bytes()).await.unwrap();
        self.writer.write_all(b"\r\n").await.unwrap();
        self.writer.flush().await.unwrap();
    }

    async fn send_raw(&mut self, s: &str) {
        self.writer.write_all(s.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }

    async fn read_reply(&mut self) -> SmtpReply {
        let mut all = Vec::new();
        loop {
            let mut line = String::new();
            let n = tokio::time::timeout(
                Duration::from_secs(2),
                self.reader.read_line(&mut line),
            )
            .await
            .expect("read reply timed out")
            .expect("read");
            assert!(n > 0, "smtp(tls) closed unexpectedly mid-reply");
            let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
            all.push(trimmed.clone());
            if trimmed.len() < 4 {
                continue;
            }
            let sep = trimmed.as_bytes()[3];
            if sep == b' ' {
                let code = trimmed[..3].to_string();
                return SmtpReply { code, last: trimmed, all };
            }
        }
    }

    async fn expect_code(&mut self, code: &str) -> SmtpReply {
        let r = self.read_reply().await;
        assert_eq!(r.code, code, "reply: {:?}", r.last);
        r
    }
}

#[derive(Debug)]
struct NoCertVerify;

impl rustls::client::danger::ServerCertVerifier for NoCertVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

#[tokio::test]
async fn ehlo_resets_pending_transaction() {
    let s = start().await;
    let mut c = Session::connect(s.smtp_addr).await;
    c.expect_code("220").await;
    c.send_line("EHLO first").await;
    c.expect_code("250").await;
    c.send_line("MAIL FROM:<a@b>").await;
    c.expect_code("250").await;
    c.send_line("EHLO again").await;
    c.expect_code("250").await;
    // After re-EHLO, transaction state is cleared.
    c.send_line("RCPT TO:<x@jalencode.com>").await;
    c.expect_code("503").await;
}
