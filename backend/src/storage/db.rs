use crate::error::{AppError, AppResult};
use crate::models::*;
use sqlx::{Row, SqlitePool};

pub struct InsertMessage<'a> {
    pub id: &'a str,
    pub local_part: &'a str,
    pub from_addr: &'a str,
    pub from_name: Option<&'a str>,
    pub to_addrs: &'a [String],
    pub subject: Option<&'a str>,
    pub preview: Option<&'a str>,
    pub text_body: Option<&'a str>,
    pub html_body: Option<&'a str>,
    pub received_at: i64,
    pub size_bytes: i64,
    pub has_html: bool,
    pub has_text: bool,
    pub raw_path: &'a str,
    pub message_id_header: Option<&'a str>,
    pub in_reply_to: Option<&'a str>,
}

#[allow(dead_code)]
pub struct AttachmentRow {
    pub id: String,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub blob_path: String,
}

pub async fn ensure_address(pool: &SqlitePool, local_part: &str) -> AppResult<bool> {
    let now = chrono::Utc::now().timestamp();
    let result = sqlx::query(
        "INSERT OR IGNORE INTO addresses (local_part, pinned, created_at) VALUES (?, 0, ?)",
    )
    .bind(local_part)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn upsert_address(
    pool: &SqlitePool,
    local_part: &str,
    label: Option<&str>,
    pinned: bool,
) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO addresses (local_part, label, pinned, created_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(local_part) DO UPDATE SET
            label = COALESCE(excluded.label, addresses.label),
            pinned = excluded.pinned",
    )
    .bind(local_part)
    .bind(label)
    .bind(if pinned { 1 } else { 0 })
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn address_exists(pool: &SqlitePool, local_part: &str) -> AppResult<bool> {
    let row = sqlx::query("SELECT 1 AS x FROM addresses WHERE local_part = ?")
        .bind(local_part)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub async fn list_addresses(pool: &SqlitePool) -> AppResult<Vec<Address>> {
    let rows = sqlx::query(
        "SELECT a.local_part, a.label, a.pinned, a.created_at,
                COUNT(m.id) AS message_count,
                MAX(m.received_at) AS last_received_at,
                COALESCE(SUM(CASE WHEN m.id IS NOT NULL AND m.read_at IS NULL THEN 1 ELSE 0 END), 0) AS unread_count
         FROM addresses a
         LEFT JOIN messages m ON m.local_part = a.local_part
         GROUP BY a.local_part
         ORDER BY a.pinned DESC,
                  COALESCE(MAX(m.received_at), a.created_at) DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Address {
            local_part: r.get("local_part"),
            label: r.get("label"),
            pinned: r.get::<i64, _>("pinned") != 0,
            created_at: r.get("created_at"),
            message_count: r.get("message_count"),
            last_received_at: r.get("last_received_at"),
            unread_count: r.get("unread_count"),
        })
        .collect())
}

pub async fn get_address(pool: &SqlitePool, local_part: &str) -> AppResult<Address> {
    let row = sqlx::query(
        "SELECT a.local_part, a.label, a.pinned, a.created_at,
                COUNT(m.id) AS message_count,
                MAX(m.received_at) AS last_received_at,
                COALESCE(SUM(CASE WHEN m.id IS NOT NULL AND m.read_at IS NULL THEN 1 ELSE 0 END), 0) AS unread_count
         FROM addresses a
         LEFT JOIN messages m ON m.local_part = a.local_part
         WHERE a.local_part = ?
         GROUP BY a.local_part",
    )
    .bind(local_part)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Address {
        local_part: row.get("local_part"),
        label: row.get("label"),
        pinned: row.get::<i64, _>("pinned") != 0,
        created_at: row.get("created_at"),
        message_count: row.get("message_count"),
        last_received_at: row.get("last_received_at"),
        unread_count: row.get("unread_count"),
    })
}

pub async fn delete_address(pool: &SqlitePool, local_part: &str) -> AppResult<()> {
    let res = sqlx::query("DELETE FROM addresses WHERE local_part = ?")
        .bind(local_part)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn insert_message(pool: &SqlitePool, m: InsertMessage<'_>) -> AppResult<()> {
    let to_json = serde_json::to_string(m.to_addrs).unwrap_or_else(|_| "[]".into());
    sqlx::query(
        "INSERT INTO messages
         (id, local_part, from_addr, from_name, to_addrs, subject, preview,
          text_body, html_body, received_at, size_bytes, has_html, has_text,
          raw_path, message_id_header, in_reply_to)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(m.id)
    .bind(m.local_part)
    .bind(m.from_addr)
    .bind(m.from_name)
    .bind(to_json)
    .bind(m.subject)
    .bind(m.preview)
    .bind(m.text_body)
    .bind(m.html_body)
    .bind(m.received_at)
    .bind(m.size_bytes)
    .bind(if m.has_html { 1 } else { 0 })
    .bind(if m.has_text { 1 } else { 0 })
    .bind(m.raw_path)
    .bind(m.message_id_header)
    .bind(m.in_reply_to)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_attachment(
    pool: &SqlitePool,
    id: &str,
    message_id: &str,
    filename: Option<&str>,
    mime_type: Option<&str>,
    size_bytes: i64,
    blob_path: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO attachments (id, message_id, filename, mime_type, size_bytes, blob_path)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(message_id)
    .bind(filename)
    .bind(mime_type)
    .bind(size_bytes)
    .bind(blob_path)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_messages(
    pool: &SqlitePool,
    address: Option<&str>,
    cursor: Option<&str>,
    limit: i64,
) -> AppResult<Vec<MessageSummary>> {
    // cursor format: "<received_at>:<id>"  (descending)
    let (cur_ts, cur_id) = match cursor {
        Some(c) => {
            let mut parts = c.splitn(2, ':');
            let ts: i64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(i64::MAX);
            let id = parts.next().unwrap_or("").to_string();
            (ts, id)
        }
        None => (i64::MAX, String::new()),
    };

    let rows = if let Some(addr) = address {
        sqlx::query(
            "SELECT id, local_part, from_addr, from_name, subject, preview,
                    received_at, size_bytes, has_html, has_text, read_at
             FROM messages
             WHERE local_part = ?
               AND (received_at < ? OR (received_at = ? AND id > ?))
             ORDER BY received_at DESC, id ASC
             LIMIT ?",
        )
        .bind(addr)
        .bind(cur_ts)
        .bind(cur_ts)
        .bind(&cur_id)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, local_part, from_addr, from_name, subject, preview,
                    received_at, size_bytes, has_html, has_text, read_at
             FROM messages
             WHERE (received_at < ? OR (received_at = ? AND id > ?))
             ORDER BY received_at DESC, id ASC
             LIMIT ?",
        )
        .bind(cur_ts)
        .bind(cur_ts)
        .bind(&cur_id)
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    Ok(rows
        .into_iter()
        .map(|r| MessageSummary {
            id: r.get("id"),
            local_part: r.get("local_part"),
            from_addr: r.get("from_addr"),
            from_name: r.get("from_name"),
            subject: r.get("subject"),
            preview: r.get("preview"),
            received_at: r.get("received_at"),
            size_bytes: r.get("size_bytes"),
            has_html: r.get::<i64, _>("has_html") != 0,
            has_text: r.get::<i64, _>("has_text") != 0,
            read: r.get::<Option<i64>, _>("read_at").is_some(),
        })
        .collect())
}

pub async fn get_message_detail(pool: &SqlitePool, id: &str) -> AppResult<MessageDetail> {
    let row = sqlx::query(
        "SELECT id, local_part, from_addr, from_name, to_addrs, subject, preview,
                text_body, html_body, received_at, size_bytes, has_html, has_text,
                read_at, message_id_header, in_reply_to
         FROM messages WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let to_addrs_json: String = row.get("to_addrs");
    let to_addrs: Vec<String> = serde_json::from_str(&to_addrs_json).unwrap_or_default();

    let summary = MessageSummary {
        id: row.get("id"),
        local_part: row.get("local_part"),
        from_addr: row.get("from_addr"),
        from_name: row.get("from_name"),
        subject: row.get("subject"),
        preview: row.get("preview"),
        received_at: row.get("received_at"),
        size_bytes: row.get("size_bytes"),
        has_html: row.get::<i64, _>("has_html") != 0,
        has_text: row.get::<i64, _>("has_text") != 0,
        read: row.get::<Option<i64>, _>("read_at").is_some(),
    };

    let attachments = sqlx::query(
        "SELECT id, filename, mime_type, size_bytes
         FROM attachments WHERE message_id = ? ORDER BY id ASC",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| AttachmentMeta {
        id: r.get("id"),
        filename: r.get("filename"),
        mime_type: r.get("mime_type"),
        size_bytes: r.get("size_bytes"),
    })
    .collect();

    Ok(MessageDetail {
        summary,
        to_addrs,
        text_body: row.get("text_body"),
        html_body: row.get("html_body"),
        message_id_header: row.get("message_id_header"),
        in_reply_to: row.get("in_reply_to"),
        attachments,
    })
}

pub async fn get_raw_path(pool: &SqlitePool, id: &str) -> AppResult<String> {
    let row = sqlx::query("SELECT raw_path FROM messages WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(row.get("raw_path"))
}

pub async fn delete_message(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let res = sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn mark_read(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let res = sqlx::query("UPDATE messages SET read_at = ? WHERE id = ? AND read_at IS NULL")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        // either already read or doesn't exist; check existence
        let exists = sqlx::query("SELECT 1 AS x FROM messages WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Err(AppError::NotFound);
        }
    }
    Ok(())
}

pub async fn get_attachment(
    pool: &SqlitePool,
    message_id: &str,
    attachment_id: &str,
) -> AppResult<AttachmentRow> {
    let row = sqlx::query(
        "SELECT id, filename, mime_type, size_bytes, blob_path
         FROM attachments WHERE message_id = ? AND id = ?",
    )
    .bind(message_id)
    .bind(attachment_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(AttachmentRow {
        id: row.get("id"),
        filename: row.get("filename"),
        mime_type: row.get("mime_type"),
        size_bytes: row.get("size_bytes"),
        blob_path: row.get("blob_path"),
    })
}

pub async fn insert_sent(
    pool: &SqlitePool,
    id: &str,
    from_local_part: &str,
    to_addrs: &[String],
    subject: &str,
    text_body: Option<&str>,
    html_body: Option<&str>,
    in_reply_to: Option<&str>,
    resend_id: Option<&str>,
) -> AppResult<()> {
    let to_json = serde_json::to_string(to_addrs).unwrap_or_else(|_| "[]".into());
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO sent_messages
         (id, from_local_part, to_addrs, subject, text_body, html_body, sent_at, in_reply_to, resend_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(from_local_part)
    .bind(to_json)
    .bind(subject)
    .bind(text_body)
    .bind(html_body)
    .bind(now)
    .bind(in_reply_to)
    .bind(resend_id)
    .execute(pool)
    .await?;
    Ok(())
}
