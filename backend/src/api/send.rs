use crate::{
    error::{AppError, AppResult},
    outbound::resend::ResendClient,
    state::AppState,
    storage::db,
};
use axum::{Json, extract::State};
use serde::Deserialize;
use ulid::Ulid;

#[derive(Deserialize)]
pub struct SendRequest {
    pub from_local_part: String,
    pub to: Vec<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub in_reply_to: Option<String>,
}

pub async fn send(
    State(state): State<AppState>,
    Json(req): Json<SendRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if req.to.is_empty() {
        return Err(AppError::BadRequest("at least one recipient required".into()));
    }
    if req.text.is_none() && req.html.is_none() {
        return Err(AppError::BadRequest("text or html required".into()));
    }

    let from_local_part = req.from_local_part.trim().to_ascii_lowercase();
    if from_local_part.is_empty() || from_local_part.contains('@') {
        return Err(AppError::BadRequest("invalid from_local_part".into()));
    }
    db::ensure_address(&state.db, &from_local_part).await?;

    let from = format!("{}@{}", from_local_part, state.config.mail.domain);
    let from_header = match &state.config.resend.default_from_name {
        Some(name) => format!("{name} <{from}>"),
        None => from.clone(),
    };

    let client = ResendClient::new(&state.config.resend.api_key, &state.config.resend.base_url);
    let resend_id = client
        .send(
            &from_header,
            &req.to,
            &req.subject,
            req.text.as_deref(),
            req.html.as_deref(),
            req.in_reply_to.as_deref(),
        )
        .await?;

    let id = Ulid::new().to_string();
    db::insert_sent(
        &state.db,
        &id,
        &from_local_part,
        &req.to,
        &req.subject,
        req.text.as_deref(),
        req.html.as_deref(),
        req.in_reply_to.as_deref(),
        Some(&resend_id),
    )
    .await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "resend_id": resend_id,
    })))
}
