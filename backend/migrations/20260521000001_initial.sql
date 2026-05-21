CREATE TABLE addresses (
    local_part  TEXT PRIMARY KEY,
    label       TEXT,
    pinned      INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE TABLE messages (
    id                 TEXT PRIMARY KEY,
    local_part         TEXT NOT NULL,
    from_addr          TEXT NOT NULL,
    from_name          TEXT,
    to_addrs           TEXT NOT NULL,
    subject            TEXT,
    preview            TEXT,
    text_body          TEXT,
    html_body          TEXT,
    received_at        INTEGER NOT NULL,
    size_bytes         INTEGER NOT NULL,
    has_html           INTEGER NOT NULL DEFAULT 0,
    has_text           INTEGER NOT NULL DEFAULT 0,
    read_at            INTEGER,
    raw_path           TEXT NOT NULL,
    message_id_header  TEXT,
    in_reply_to        TEXT
);
CREATE INDEX idx_messages_local_part_received ON messages(local_part, received_at DESC);
CREATE INDEX idx_messages_received_at_id      ON messages(received_at DESC, id);
CREATE INDEX idx_messages_message_id_header   ON messages(message_id_header);

CREATE TABLE attachments (
    id          TEXT PRIMARY KEY,
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename    TEXT,
    mime_type   TEXT,
    size_bytes  INTEGER NOT NULL,
    blob_path   TEXT NOT NULL
);
CREATE INDEX idx_attachments_message_id ON attachments(message_id);

CREATE TABLE sent_messages (
    id              TEXT PRIMARY KEY,
    from_local_part TEXT NOT NULL,
    to_addrs        TEXT NOT NULL,
    subject         TEXT,
    text_body       TEXT,
    html_body       TEXT,
    sent_at         INTEGER NOT NULL,
    in_reply_to     TEXT,
    resend_id       TEXT
);
CREATE INDEX idx_sent_from_local_part ON sent_messages(from_local_part, sent_at DESC);
