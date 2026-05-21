use crate::error::{AppError, AppResult};
use crate::events::Event;
use crate::state::AppState;
use crate::storage::blobs::BlobStore;
use crate::storage::db;
use mail_parser::{Address, MessageParser, MimeHeaders};
use ulid::Ulid;

pub async fn ingest_raw(state: &AppState, raw: &[u8]) -> AppResult<String> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| AppError::BadRequest("could not parse RFC822 message".into()))?;

    let domain = state.config.mail.domain.to_ascii_lowercase();

    let to_addrs = collect_addresses(msg.to());

    let local_part = to_addrs
        .iter()
        .find_map(|a| {
            let (lp, dom) = a.split_once('@')?;
            if dom.eq_ignore_ascii_case(&domain) {
                Some(lp.to_ascii_lowercase())
            } else {
                None
            }
        })
        .ok_or_else(|| AppError::BadRequest(format!("no recipient on {domain}")))?;

    if !state.config.mail.catch_all {
        let exists = db::address_exists(&state.db, &local_part).await?;
        if !exists {
            return Err(AppError::BadRequest(format!(
                "unknown address: {local_part}"
            )));
        }
    }

    db::ensure_address(&state.db, &local_part).await?;

    let id = Ulid::new().to_string();
    let blobs = BlobStore::new(&state.data_dir);
    let raw_path = blobs.write_eml(&id, raw).await?;

    let (from_addr, from_name) = first_from(msg.from());
    let subject = msg.subject().map(|s| s.to_string());
    let message_id_header = msg.message_id().map(|s| s.to_string());
    let in_reply_to = msg
        .in_reply_to()
        .as_text_list()
        .and_then(|v| v.first().map(|s| s.to_string()));

    let text_body = msg.body_text(0).map(|c| c.into_owned());
    let html_body = msg.body_html(0).map(|c| c.into_owned());
    let has_text = text_body.is_some();
    let has_html = html_body.is_some();
    let preview = text_body
        .as_ref()
        .map(|t| t.chars().take(240).collect::<String>().trim().to_string());

    let received_at = chrono::Utc::now().timestamp();
    let size_bytes = raw.len() as i64;

    db::insert_message(
        &state.db,
        db::InsertMessage {
            id: &id,
            local_part: &local_part,
            from_addr: &from_addr,
            from_name: from_name.as_deref(),
            to_addrs: &to_addrs,
            subject: subject.as_deref(),
            preview: preview.as_deref(),
            text_body: text_body.as_deref(),
            html_body: html_body.as_deref(),
            received_at,
            size_bytes,
            has_html,
            has_text,
            raw_path: raw_path.to_string_lossy().as_ref(),
            message_id_header: message_id_header.as_deref(),
            in_reply_to: in_reply_to.as_deref(),
        },
    )
    .await?;

    for att in msg.attachments() {
        let aid = Ulid::new().to_string();
        let filename = att.attachment_name().map(|s| s.to_string());
        let mime_type = att
            .content_type()
            .map(|ct| match ct.subtype() {
                Some(sub) => format!("{}/{}", ct.ctype(), sub),
                None => ct.ctype().to_string(),
            });
        let bytes = att.contents();
        let path = blobs.write_attachment(&id, &aid, bytes).await?;
        db::insert_attachment(
            &state.db,
            &aid,
            &id,
            filename.as_deref(),
            mime_type.as_deref(),
            bytes.len() as i64,
            path.to_string_lossy().as_ref(),
        )
        .await?;
    }

    state.events.send(Event::NewMessage {
        id: id.clone(),
        local_part,
    });

    Ok(id)
}

fn collect_addresses(addr: Option<&Address<'_>>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(a) = addr {
        for one in a.iter() {
            if let Some(addr) = one.address() {
                out.push(addr.to_string());
            }
        }
    }
    out
}

fn first_from(addr: Option<&Address<'_>>) -> (String, Option<String>) {
    if let Some(a) = addr {
        if let Some(one) = a.iter().next() {
            let addr = one.address().unwrap_or("").to_string();
            let name = one.name().map(|n| n.to_string());
            return (addr, name);
        }
    }
    (String::new(), None)
}
