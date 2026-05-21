use crate::{
    error::{AppError, AppResult},
    events::Event,
    models::{MessageDetail, MessageSummary},
    state::AppState,
    storage::{blobs::BlobStore, db},
};
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ListQuery {
    pub address: Option<String>,
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 {
    50
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<MessageSummary>>> {
    let limit = q.limit.clamp(1, 200);
    let msgs = db::list_messages(&state.db, q.address.as_deref(), q.cursor.as_deref(), limit).await?;
    Ok(Json(msgs))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<MessageDetail>> {
    Ok(Json(db::get_message_detail(&state.db, &id).await?))
}

pub async fn delete_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let raw_path = db::get_raw_path(&state.db, &id).await?;
    db::delete_message(&state.db, &id).await?;
    let blobs = BlobStore::new(&state.data_dir);
    let _ = blobs
        .delete_message_blobs(&id, &std::path::PathBuf::from(raw_path))
        .await;
    state.events.send(Event::MessageDeleted { id: id.clone() });
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn mark_read(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    db::mark_read(&state.db, &id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn raw_eml(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let path = db::get_raw_path(&state.db, &id).await?;
    let bytes = tokio::fs::read(&path).await.map_err(AppError::from)?;
    let disposition = format!("attachment; filename=\"{id}.eml\"");
    Ok(build_file_response(
        "message/rfc822",
        &disposition,
        bytes,
    ))
}

pub async fn attachment(
    State(state): State<AppState>,
    Path((message_id, attachment_id)): Path<(String, String)>,
) -> AppResult<Response> {
    let att = db::get_attachment(&state.db, &message_id, &attachment_id).await?;
    let bytes = tokio::fs::read(&att.blob_path).await.map_err(AppError::from)?;
    let mime = att
        .mime_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    let filename = att.filename.unwrap_or_else(|| attachment_id.clone());
    let disposition = format!(
        "attachment; filename=\"{}\"",
        filename.replace('"', "")
    );
    Ok(build_file_response(mime, &disposition, bytes))
}

fn build_file_response(content_type: &str, disposition: &str, bytes: Vec<u8>) -> Response {
    let mut resp = Response::new(Body::from(bytes));
    *resp.status_mut() = StatusCode::OK;
    if let Ok(v) = HeaderValue::from_str(content_type) {
        resp.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = HeaderValue::from_str(disposition) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    resp.into_response()
}
