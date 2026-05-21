use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub local_part: String,
    pub label: Option<String>,
    pub pinned: bool,
    pub created_at: i64,
    pub message_count: i64,
    pub last_received_at: Option<i64>,
    pub unread_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    pub id: String,
    pub local_part: String,
    pub from_addr: String,
    pub from_name: Option<String>,
    pub subject: Option<String>,
    pub preview: Option<String>,
    pub received_at: i64,
    pub size_bytes: i64,
    pub has_html: bool,
    pub has_text: bool,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDetail {
    #[serde(flatten)]
    pub summary: MessageSummary,
    pub to_addrs: Vec<String>,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub message_id_header: Option<String>,
    pub in_reply_to: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    pub id: String,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
}
