# Temp Mail

Self-hosted temporary mail. Receive mail at any address on a domain you control, read it in a native iOS / macOS app, and reply through Resend.

> Personal-use project. The expectation is that whoever runs it owns the domain and the backend. There's no signup, no multi-tenancy, one operator per instance.

```
                   ┌─────────────────────────────────────────┐
   Sender ─► MX ─► │  Backend (backend/)                     │
   (your home     │                                          │
    server IP)    │  • SMTP listener on :25                  │
                  │      receives & parses incoming mail     │
                  │  • HTTP API (axum)                       │
                  │      /v1/messages, /v1/addresses         │
                  │      /v1/events  — SSE for live updates  │
                  │      /v1/send    — outbound via Resend   │
                  │  • SQLite (WAL)                          │
                  │      messages, addresses, attachments    │
                  └────────────────┬────────────────────────┘
                                   │  HTTPS + Bearer token + SSE
                                   ▼
                   ┌─────────────────────────────────────────┐
                   │  Swift app (ios/)                       │
                   │  SwiftUI + Liquid Glass, iOS+macOS 26   │
                   └─────────────────────────────────────────┘
```

## Repo layout

```
temp-mail/
├── backend/      Rust + axum + SQLite. The actual server (HTTP + SMTP).
├── ios/          SwiftUI app, builds for iOS and macOS.
└── README.md     ← you are here
```

Each component has its own README with deeper details.

## Quick start (local)

You'll be running two things: the backend and the iOS / macOS app.

### 1. Backend

```bash
cd backend
cp config.example.toml config.toml
# edit config.toml — set api_token, domain, resend.api_key. For local dev you can
# comment out [smtp] and just exercise the HTTP API; for receiving real mail,
# bind [smtp].listen to "0.0.0.0:25" on a host with port 25 open.
cargo run --release
# HTTP listens on whatever [server].listen is (default 127.0.0.1:8787).
# SMTP, if configured, listens on [smtp].listen.
```

The backend creates `./data/` next to itself and writes:
- `tempmail.db` (SQLite, WAL mode)
- `eml/<id>.eml` (raw RFC822)
- `attachments/<message-id>/<attachment-id>`

Migrations run automatically at startup.

### 2. Inject a test message without standing up SMTP

The same `/v1/ingest` HMAC-signed HTTP endpoint that an external worker would use is also handy for local testing:

```bash
cat > /tmp/m.eml <<'EOF'
From: A. Sender <sender@example.com>
To: hello@yourdomain.com
Subject: Hello there
Date: Wed, 21 May 2026 12:00:00 +0000

This is a test.
EOF
SIG=$(openssl dgst -sha256 -hmac "<ingest_hmac_secret>" /tmp/m.eml | awk '{print $2}')
curl -sS -X POST -H "X-Signature: $SIG" --data-binary @/tmp/m.eml \
  http://127.0.0.1:8787/v1/ingest
```

### 3. iOS / macOS app

Requires Xcode 17+ (iOS 26 / macOS 26 SDK) and [XcodeGen](https://github.com/yonaskolb/XcodeGen).

```bash
cd ios
brew install xcodegen        # one-time
xcodegen generate
open TempMail.xcodeproj
# pick TempMail-iOS or TempMail-macOS, ⌘R
```

On first launch you'll see a pairing screen. Enter:
- **Server URL**: `http://127.0.0.1:8787`
- **API Token**: the `api_token` you set in `config.toml`

The iOS Simulator can reach the host's `127.0.0.1`. The Info.plist has an `NSAllowsLocalNetworking` exception so the http:// localhost URL works in development.

## Going live

The setup is direct-SMTP-to-your-server. Things you need:

1. **A public IP and inbound port 25 open** to the host running the backend. If you're behind a residential ISP, confirm 25 isn't blocked and that your router forwards it.
2. **A static IP, or dynamic DNS to a stable hostname.** Mail servers retry, but they need a destination.
3. **MX record** for the domain pointing at that hostname:
   ```
   yourdomain.com.   MX  10  mail.yourdomain.com.
   mail.yourdomain.com.  A   <your public IP>
   ```
4. **PTR (reverse DNS)** for your IP resolving to that hostname. Many ISPs let you configure this from their portal; some don't. Without it, Gmail and others may treat your mail as suspicious or refuse it.
5. **SPF and DKIM records** for sending through Resend. Resend's dashboard generates the exact 3 DKIM CNAMEs + the SPF TXT record. With your own SMTP for receiving plus Resend for sending, the SPF looks like:
   ```
   v=spf1 include:_spf.resend.com ~all
   ```
6. **Optional: DMARC** — `v=DMARC1; p=none; rua=mailto:postmaster@yourdomain.com` to start permissive.

Then in `backend/config.toml`:
```toml
[smtp]
listen = "0.0.0.0:25"
hostname = "mail.yourdomain.com"
```

Restart and test by sending mail from another provider to anything `@yourdomain.com`.

### Port-25 reality check

Some residential ISPs block inbound :25 entirely; others don't. If you've delivered mail to a residential server before, you're already past this hurdle. Otherwise, run a quick test:

```bash
# From any other host (e.g. a phone on cellular):
telnet your-public-ip 25
# If it connects and you see the banner, you're good.
```

The other gotcha is some senders (notably Gmail) will deliver but mark mail as spam if your IP is on residential blocklists. PTR + SPF + DKIM + DMARC alignment is what mostly fixes that.

## Tests

```bash
# Backend — 72 tests total:
#   • HTTP API: auth, addresses, messages, HMAC /v1/ingest
#   • Pipeline edge cases (no subject, multi-recipient, html-only, attachments)
#   • Outbound /v1/send against wiremock-mocked Resend
#   • SSE end-to-end through a real TCP server
#   • SMTP RFC 5321 conformance suite (35 tests): banner, EHLO/HELO, state
#     machine, address parsing, DATA + dot-stuffing, oversize handling,
#     pipelining, RSET / VRFY / EXPN / NOOP / HELP, error codes, plus
#     STARTTLS handshake + transaction over TLS (RFC 3207)
cd backend && cargo test

# Swift — 12 unit tests (models, JSON decoding, SSE payload parsing)
cd ios && xcodebuild -project TempMail.xcodeproj -scheme TempMail-macOS \
  -destination 'platform=macOS' test
```

## Tech stack

| Component   | Stack |
|-------------|-------|
| Backend     | Rust, axum 0.8, sqlx + SQLite (WAL), mail-parser, custom tokio-based SMTP receiver, reqwest, rustls |
| iOS / macOS | SwiftUI, Observation, Liquid Glass, Keychain, WKWebView for HTML mail |
| Outbound    | Resend |
| Inbound     | Direct SMTP on port 25, no intermediate provider |

## Design choices worth knowing

- **Catch-all by default.** Any `*@yourdomain` is accepted; addresses materialize when mail arrives. `mail.catch_all = false` in config flips it to allowlist mode.
- **No retention policy.** Mail is kept forever. Storage is on you. Manual delete from the app removes the row + raw `.eml` + attachments.
- **Single bearer token for the app.** No user accounts, no sessions. Token is stored in the Keychain on the device. Rotate by changing `auth.api_token` and signing back in.
- **Two ways to ingest.** The SMTP receiver is the production path. The HMAC-signed `/v1/ingest` HTTP endpoint is kept around for testing and as an escape hatch (e.g., if you ever want to put a webhook-based provider in front).
- **SSE for live updates.** The app subscribes to `/v1/events` and gets `new_message` / `message_deleted` / `address_added` events without polling.
- **STARTTLS optional.** Inbound SMTP works plaintext on :25 by default. If you set `[smtp].tls_cert` and `tls_key` (PEM paths), the server advertises STARTTLS in EHLO and accepts TLS upgrades. Recommended for production — Gmail and others rank TLS-protected delivery higher.

## Status / what's not here

- No push notifications. Live updates while the app is open work via SSE; APNs is future work.
- No multi-user / sharing.
- No web UI — native only.
- Worker-based ingress (Cloudflare etc.) was scaffolded earlier in the repo's history and removed in favor of direct SMTP. If you ever want it back, the `/v1/ingest` HMAC endpoint is still there.
