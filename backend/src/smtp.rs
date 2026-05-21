//! SMTP receiver. RFC 5321 with the 8BITMIME (RFC 6152), SIZE (RFC 1870),
//! PIPELINING (RFC 2920), and STARTTLS (RFC 3207) extensions. No AUTH —
//! we're a delivery endpoint, not a relay or submission server.
//!
//! Design notes:
//! - Bytes in, bytes out. RFC 5321 commands are ASCII but DATA contents are
//!   arbitrary 8-bit (we advertise 8BITMIME). We never `String`-decode
//!   message bodies.
//! - State machine: New → Greeted → MailFrom → Rcpt → (DATA terminator)
//!   → Greeted. RSET returns to Greeted; QUIT exits. After STARTTLS we
//!   discard all session state and restart the SMTP loop on a TLS-wrapped
//!   socket, per RFC 3207 §4.2.
//! - Line-length enforcement protects against memory exhaustion via
//!   slowloris-style attacks: 512 octets for commands (RFC 5321 §4.5.3.1.4),
//!   1000 octets for DATA lines (§4.5.3.1.6).
//! - Trace header (`Received:`) is prepended to every accepted message per
//!   RFC 5321 §4.4 before handing the raw bytes to the parse pipeline.

use crate::config::SmtpConfig;
use crate::pipeline;
use crate::state::AppState;
use anyhow::{Context, Result};
use chrono::Utc;
use rustls::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

const MAX_COMMAND_LINE: usize = 512;
const MAX_DATA_LINE: usize = 1000;
const SESSION_TIMEOUT: Duration = Duration::from_secs(300);

pub async fn run(cfg: SmtpConfig, state: AppState) -> Result<()> {
    let listener = TcpListener::bind(&cfg.listen).await?;
    let tls = load_tls_acceptor(&cfg)?;
    tracing::info!(
        addr = %cfg.listen,
        hostname = %cfg.hostname,
        tls = tls.is_some(),
        "SMTP listening"
    );
    serve(listener, cfg, state, tls).await
}

pub async fn serve(
    listener: TcpListener,
    cfg: SmtpConfig,
    state: AppState,
    tls: Option<Arc<TlsAcceptor>>,
) -> Result<()> {
    loop {
        let (socket, peer) = listener.accept().await?;
        let state = state.clone();
        let cfg = cfg.clone();
        let tls = tls.clone();
        tokio::spawn(async move {
            let res = tokio::time::timeout(
                SESSION_TIMEOUT,
                handle_session(socket, peer, state, cfg, tls),
            )
            .await;
            match res {
                Err(_) => tracing::info!(%peer, "smtp session timed out"),
                Ok(Err(e)) => tracing::warn!(%peer, error = ?e, "smtp session error"),
                Ok(Ok(())) => {}
            }
        });
    }
}

pub fn load_tls_acceptor(cfg: &SmtpConfig) -> Result<Option<Arc<TlsAcceptor>>> {
    match (&cfg.tls_cert, &cfg.tls_key) {
        (Some(cert), Some(key)) => {
            let config = build_server_config(cert, key)
                .with_context(|| format!("loading SMTP TLS cert {cert} / key {key}"))?;
            Ok(Some(Arc::new(TlsAcceptor::from(Arc::new(config)))))
        }
        (None, None) => Ok(None),
        _ => anyhow::bail!("[smtp].tls_cert and tls_key must both be set, or both unset"),
    }
}

fn build_server_config(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
    build_server_config_from_paths(Path::new(cert_path), Path::new(key_path))
}

pub fn build_server_config_from_paths(cert_path: &Path, key_path: &Path) -> Result<ServerConfig> {
    let cert_bytes = std::fs::read(cert_path)
        .with_context(|| format!("reading TLS cert at {}", cert_path.display()))?;
    let key_bytes = std::fs::read(key_path)
        .with_context(|| format!("reading TLS key at {}", key_path.display()))?;
    build_server_config_from_pem(&cert_bytes, &key_bytes)
}

pub fn build_server_config_from_pem(cert_pem: &[u8], key_pem: &[u8]) -> Result<ServerConfig> {
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut &cert_pem[..])
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("parsing TLS cert chain")?;
    if certs.is_empty() {
        anyhow::bail!("TLS cert file contained no certificates");
    }
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut &key_pem[..])
        .context("parsing TLS private key")?
        .ok_or_else(|| anyhow::anyhow!("TLS key file contained no private key"))?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    Ok(config)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Fresh,
    Greeted,
    MailFrom,
    Rcpt,
}

enum Outcome {
    Quit,
    Upgrade,
    Eof,
}

async fn handle_session(
    stream: TcpStream,
    peer: SocketAddr,
    state: AppState,
    cfg: SmtpConfig,
    tls: Option<Arc<TlsAcceptor>>,
) -> Result<()> {
    let (read, write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut writer = BufWriter::new(write);

    write_line(&mut writer, &format!("220 {} ESMTP ready", cfg.hostname)).await?;

    let outcome = run_smtp(
        &mut reader,
        &mut writer,
        &state,
        &cfg,
        peer,
        tls.is_some(),
        false,
    )
    .await?;

    if matches!(outcome, Outcome::Upgrade) {
        let acceptor = tls.expect("upgrade requested without TLS acceptor");

        let read_half = reader.into_inner();
        let write_half = writer.into_inner();
        let tcp = read_half
            .reunite(write_half)
            .map_err(|e| anyhow::anyhow!("could not reunite tcp halves: {e}"))?;

        let tls_stream = acceptor
            .accept(tcp)
            .await
            .context("TLS handshake failed")?;

        let (tls_read, tls_write) = tokio::io::split(tls_stream);
        let mut reader = BufReader::new(tls_read);
        let mut writer = BufWriter::new(tls_write);

        // STARTTLS resets all session state; client must re-EHLO.
        let _ = run_smtp(
            &mut reader,
            &mut writer,
            &state,
            &cfg,
            peer,
            false, // STARTTLS not advertised again
            true,  // already TLS
        )
        .await?;
    }

    Ok(())
}

async fn run_smtp<R, W>(
    reader: &mut BufReader<R>,
    writer: &mut BufWriter<W>,
    state: &AppState,
    cfg: &SmtpConfig,
    peer: SocketAddr,
    tls_available: bool,
    tls_active: bool,
) -> Result<Outcome>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let max_size = state.config.mail.max_message_bytes;
    let domain = state.config.mail.domain.to_ascii_lowercase();

    let mut st = State::Fresh;
    let mut helo_name: Option<String> = None;
    let mut mail_from: Option<String> = None;
    let mut rcpts: Vec<String> = Vec::new();
    let mut is_ehlo = false;

    loop {
        let line = match read_line(reader, MAX_COMMAND_LINE).await {
            ReadLine::Eof => return Ok(Outcome::Eof),
            ReadLine::TooLong => {
                write_line(writer, "500 5.5.6 line too long").await?;
                continue;
            }
            ReadLine::Line(b) => b,
        };

        let cmd_line = String::from_utf8_lossy(&line);
        let cmd_line = cmd_line.trim_end_matches(['\r', '\n']);

        let (verb, rest) = split_verb(cmd_line);
        let verb_upper = verb.to_ascii_uppercase();

        match verb_upper.as_str() {
            "QUIT" => {
                if !rest.is_empty() {
                    write_line(writer, "501 5.5.4 QUIT takes no arguments").await?;
                    continue;
                }
                write_line(writer, "221 2.0.0 bye").await?;
                return Ok(Outcome::Quit);
            }

            "HELO" => {
                let arg = rest.trim();
                if arg.is_empty() {
                    write_line(writer, "501 5.5.4 HELO requires a domain").await?;
                    continue;
                }
                helo_name = Some(arg.to_string());
                is_ehlo = false;
                st = State::Greeted;
                mail_from = None;
                rcpts.clear();
                write_line(writer, &format!("250 {} hello {arg}", cfg.hostname)).await?;
            }

            "EHLO" => {
                let arg = rest.trim();
                if arg.is_empty() {
                    write_line(writer, "501 5.5.4 EHLO requires a domain").await?;
                    continue;
                }
                helo_name = Some(arg.to_string());
                is_ehlo = true;
                st = State::Greeted;
                mail_from = None;
                rcpts.clear();
                // Multi-line 250: keep `250 <last>` as the terminator.
                let mut lines: Vec<String> =
                    vec![format!("{} hello {arg}", cfg.hostname), format!("SIZE {max_size}")];
                lines.push("8BITMIME".into());
                lines.push("PIPELINING".into());
                lines.push("ENHANCEDSTATUSCODES".into());
                if tls_available && !tls_active {
                    lines.push("STARTTLS".into());
                }
                lines.push("HELP".into());
                let last = lines.len() - 1;
                for (i, body) in lines.iter().enumerate() {
                    let sep = if i == last { ' ' } else { '-' };
                    write_line(writer, &format!("250{sep}{body}")).await?;
                }
            }

            "STARTTLS" => {
                if !tls_available {
                    write_line(writer, "502 5.5.1 STARTTLS not available").await?;
                    continue;
                }
                if tls_active {
                    write_line(writer, "503 5.5.1 already using TLS").await?;
                    continue;
                }
                if !rest.is_empty() {
                    write_line(writer, "501 5.5.4 STARTTLS takes no arguments").await?;
                    continue;
                }
                write_line(writer, "220 2.0.0 ready to start TLS").await?;
                writer.flush().await?;
                return Ok(Outcome::Upgrade);
            }

            "AUTH" => {
                write_line(writer, "502 5.5.1 AUTH not implemented").await?;
            }

            "RSET" => {
                if !rest.is_empty() {
                    write_line(writer, "501 5.5.4 RSET takes no arguments").await?;
                    continue;
                }
                mail_from = None;
                rcpts.clear();
                if st != State::Fresh {
                    st = State::Greeted;
                }
                write_line(writer, "250 2.0.0 reset").await?;
            }

            "NOOP" => {
                write_line(writer, "250 2.0.0 ok").await?;
            }

            "VRFY" => {
                write_line(writer, "252 2.1.5 cannot verify").await?;
            }

            "EXPN" => {
                write_line(writer, "252 2.1.5 cannot expand").await?;
            }

            "HELP" => {
                write_line(
                    writer,
                    "214-Commands: HELO EHLO MAIL RCPT DATA RSET NOOP VRFY EXPN HELP QUIT",
                )
                .await?;
                write_line(writer, "214 2.0.0 end of HELP").await?;
            }

            "MAIL" => {
                if st == State::Fresh {
                    write_line(writer, "503 5.5.1 say HELO/EHLO first").await?;
                    continue;
                }
                if st != State::Greeted {
                    write_line(writer, "503 5.5.1 nested MAIL command").await?;
                    continue;
                }
                match parse_path_command(rest, "FROM:") {
                    Ok((path, _params)) => {
                        if let Some(addr) = &path {
                            if addr.len() > 256 {
                                write_line(writer, "501 5.5.4 reverse-path too long").await?;
                                continue;
                            }
                        }
                        mail_from = Some(path.unwrap_or_default());
                        rcpts.clear();
                        st = State::MailFrom;
                        write_line(writer, "250 2.1.0 sender ok").await?;
                    }
                    Err(reply) => {
                        write_line(writer, reply).await?;
                    }
                }
            }

            "RCPT" => {
                match st {
                    State::Fresh => {
                        write_line(writer, "503 5.5.1 say HELO/EHLO first").await?;
                        continue;
                    }
                    State::Greeted => {
                        write_line(writer, "503 5.5.1 need MAIL FROM first").await?;
                        continue;
                    }
                    _ => {}
                }
                match parse_path_command(rest, "TO:") {
                    Ok((Some(addr), _params)) => {
                        if addr.len() > 256 {
                            write_line(writer, "501 5.5.4 forward-path too long").await?;
                            continue;
                        }
                        let (lp, dom) = match addr.rsplit_once('@') {
                            Some(parts) => parts,
                            None => {
                                write_line(writer, "553 5.1.3 bad address").await?;
                                continue;
                            }
                        };
                        if lp.is_empty() || lp.len() > 64 {
                            write_line(writer, "553 5.1.3 bad local-part").await?;
                            continue;
                        }
                        if !dom.eq_ignore_ascii_case(&domain) {
                            write_line(
                                writer,
                                "550 5.7.1 relaying denied: recipient not on this domain",
                            )
                            .await?;
                            continue;
                        }
                        rcpts.push(addr);
                        st = State::Rcpt;
                        write_line(writer, "250 2.1.5 recipient ok").await?;
                    }
                    Ok((None, _)) => {
                        write_line(writer, "501 5.5.4 RCPT requires an address").await?;
                    }
                    Err(reply) => {
                        write_line(writer, reply).await?;
                    }
                }
            }

            "DATA" => {
                match st {
                    State::Fresh => {
                        write_line(writer, "503 5.5.1 say HELO/EHLO first").await?;
                        continue;
                    }
                    State::Greeted | State::MailFrom => {
                        write_line(writer, "503 5.5.1 no recipients").await?;
                        continue;
                    }
                    State::Rcpt => {}
                }
                if !rest.is_empty() {
                    write_line(writer, "501 5.5.4 DATA takes no arguments").await?;
                    continue;
                }
                write_line(writer, "354 end with <CR><LF>.<CR><LF>").await?;

                let outcome = read_data(reader, max_size).await;
                let body = match outcome {
                    DataOutcome::Body(b) => b,
                    DataOutcome::TooLarge => {
                        write_line(writer, "552 5.3.4 message too large").await?;
                        mail_from = None;
                        rcpts.clear();
                        st = State::Greeted;
                        continue;
                    }
                    DataOutcome::LineTooLong => {
                        write_line(writer, "500 5.5.6 line too long in DATA").await?;
                        mail_from = None;
                        rcpts.clear();
                        st = State::Greeted;
                        continue;
                    }
                    DataOutcome::Eof => return Ok(Outcome::Eof),
                };

                let received = received_header(
                    helo_name.as_deref().unwrap_or("unknown"),
                    &peer,
                    &cfg.hostname,
                    if is_ehlo { "ESMTP" } else { "SMTP" },
                    rcpts.first().map(|s| s.as_str()),
                    tls_active,
                );
                let mut full = Vec::with_capacity(received.len() + body.len());
                full.extend_from_slice(received.as_bytes());
                full.extend_from_slice(&body);

                match pipeline::ingest_raw(state, &full).await {
                    Ok(id) => {
                        tracing::info!(
                            msg_id = %id,
                            %peer,
                            tls = tls_active,
                            helo = %helo_name.as_deref().unwrap_or(""),
                            from = %mail_from.as_deref().unwrap_or(""),
                            rcpts = ?rcpts,
                            "smtp accepted"
                        );
                        write_line(writer, &format!("250 2.0.0 ok {id}")).await?;
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, %peer, "smtp pipeline rejected");
                        write_line(writer, "554 5.6.0 message rejected").await?;
                    }
                }
                mail_from = None;
                rcpts.clear();
                st = State::Greeted;
            }

            "" => {
                continue;
            }

            _ => {
                write_line(writer, "500 5.5.2 command unrecognized").await?;
            }
        }
    }
}

// ─── command parsing ────────────────────────────────────────────────────────

fn split_verb(s: &str) -> (&str, &str) {
    let mut idx = 0;
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_whitespace() {
            return (&s[..i], &s[i..]);
        }
        idx = i + ch.len_utf8();
    }
    (&s[..idx], "")
}

fn parse_path_command(
    rest: &str,
    keyword: &str,
) -> std::result::Result<(Option<String>, Vec<(String, String)>), &'static str> {
    let s = rest.trim_start();
    let stripped = match strip_prefix_ascii_case(s, keyword) {
        Some(x) => x,
        None => return Err("501 5.5.4 syntax: expected FROM:<addr> or TO:<addr>"),
    };
    let s = stripped.trim_start();
    let (path_token, params_str) = if let Some(rest) = s.strip_prefix('<') {
        let close = match rest.find('>') {
            Some(p) => p,
            None => return Err("501 5.5.4 unterminated angle-bracket address"),
        };
        let path = &rest[..close];
        let after = rest[close + 1..].trim_start();
        (path.to_string(), after.to_string())
    } else {
        let end = s.find(char::is_whitespace).unwrap_or(s.len());
        let path = &s[..end];
        let after = s[end..].trim_start();
        (path.to_string(), after.to_string())
    };

    let path = if path_token.is_empty() {
        None
    } else {
        Some(path_token)
    };

    let mut params = Vec::new();
    for token in params_str.split_ascii_whitespace() {
        if let Some((k, v)) = token.split_once('=') {
            params.push((k.to_ascii_uppercase(), v.to_string()));
        } else {
            params.push((token.to_ascii_uppercase(), String::new()));
        }
    }
    Ok((path, params))
}

fn strip_prefix_ascii_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() < prefix.len() {
        return None;
    }
    let (head, tail) = s.split_at(prefix.len());
    if head.eq_ignore_ascii_case(prefix) {
        Some(tail)
    } else {
        None
    }
}

// ─── line reading ───────────────────────────────────────────────────────────

enum ReadLine {
    Line(Vec<u8>),
    TooLong,
    Eof,
}

async fn read_line<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    max: usize,
) -> ReadLine {
    let mut buf = Vec::with_capacity(64);
    let mut byte = [0u8; 1];
    loop {
        let n = match reader.read(&mut byte).await {
            Ok(n) => n,
            Err(_) => return ReadLine::Eof,
        };
        if n == 0 {
            if buf.is_empty() {
                return ReadLine::Eof;
            }
            return ReadLine::Line(buf);
        }
        if buf.len() >= max {
            loop {
                let n = match reader.read(&mut byte).await {
                    Ok(n) => n,
                    Err(_) => return ReadLine::TooLong,
                };
                if n == 0 || byte[0] == b'\n' {
                    return ReadLine::TooLong;
                }
            }
        }
        buf.push(byte[0]);
        if byte[0] == b'\n' {
            return ReadLine::Line(buf);
        }
    }
}

enum DataOutcome {
    Body(Vec<u8>),
    TooLarge,
    LineTooLong,
    Eof,
}

async fn read_data<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    max_size: usize,
) -> DataOutcome {
    let mut body: Vec<u8> = Vec::with_capacity(8192);
    loop {
        let line = match read_line(reader, MAX_DATA_LINE).await {
            ReadLine::Eof => return DataOutcome::Eof,
            ReadLine::TooLong => {
                let _ = drain_until_dot(reader).await;
                return DataOutcome::LineTooLong;
            }
            ReadLine::Line(b) => b,
        };
        if line == b".\r\n" || line == b".\n" {
            return DataOutcome::Body(body);
        }
        let chunk: &[u8] = if line.first() == Some(&b'.') {
            &line[1..]
        } else {
            &line
        };
        if body.len() + chunk.len() > max_size {
            let _ = drain_until_dot(reader).await;
            return DataOutcome::TooLarge;
        }
        body.extend_from_slice(chunk);
    }
}

async fn drain_until_dot<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> std::io::Result<()> {
    loop {
        let line = match read_line(reader, MAX_DATA_LINE).await {
            ReadLine::Eof => return Ok(()),
            ReadLine::TooLong => continue,
            ReadLine::Line(b) => b,
        };
        if line == b".\r\n" || line == b".\n" {
            return Ok(());
        }
    }
}

// ─── trace ──────────────────────────────────────────────────────────────────

fn received_header(
    helo: &str,
    peer: &SocketAddr,
    hostname: &str,
    protocol: &str,
    for_addr: Option<&str>,
    tls: bool,
) -> String {
    let now = Utc::now().to_rfc2822();
    let with = if tls {
        format!("{protocol}S")
    } else {
        protocol.to_string()
    };
    match for_addr {
        Some(addr) => format!(
            "Received: from {helo} ([{ip}])\r\n        by {hostname} with {with};\r\n        for <{addr}>;\r\n        {now}\r\n",
            ip = peer.ip(),
        ),
        None => format!(
            "Received: from {helo} ([{ip}])\r\n        by {hostname} with {with};\r\n        {now}\r\n",
            ip = peer.ip(),
        ),
    }
}

// ─── writing ────────────────────────────────────────────────────────────────

async fn write_line<W: AsyncWrite + Unpin>(
    w: &mut BufWriter<W>,
    line: &str,
) -> Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\r\n").await?;
    w.flush().await?;
    Ok(())
}
