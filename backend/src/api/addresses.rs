use crate::{
    error::{AppError, AppResult},
    events::Event,
    models::Address,
    state::AppState,
    storage::db,
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Address>>> {
    Ok(Json(db::list_addresses(&state.db).await?))
}

#[derive(Deserialize)]
pub struct CreateAddress {
    pub local_part: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub pinned: bool,
}

pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateAddress>,
) -> AppResult<Json<Address>> {
    let local_part = req.local_part.trim().to_ascii_lowercase();
    if local_part.is_empty() || local_part.contains('@') {
        return Err(AppError::BadRequest("invalid local_part".into()));
    }
    db::upsert_address(&state.db, &local_part, req.label.as_deref(), req.pinned).await?;
    state.events.send(Event::AddressAdded {
        local_part: local_part.clone(),
    });
    Ok(Json(db::get_address(&state.db, &local_part).await?))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(local_part): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    db::delete_address(&state.db, &local_part).await?;
    state.events.send(Event::AddressRemoved {
        local_part: local_part.clone(),
    });
    Ok(Json(serde_json::json!({ "deleted": local_part })))
}
