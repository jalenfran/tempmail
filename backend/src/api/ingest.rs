use crate::{error::AppError, pipeline, state::AppState};
use axum::{Json, extract::State, http::HeaderMap};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub async fn ingest(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, AppError> {
    let sig = headers
        .get("x-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let expected = compute_hmac(state.config.auth.ingest_hmac_secret.as_bytes(), &body);
    let sig_bytes = hex::decode(sig.trim()).map_err(|_| AppError::Unauthorized)?;

    if sig_bytes.ct_eq(expected.as_slice()).unwrap_u8() != 1 {
        return Err(AppError::Unauthorized);
    }

    if body.len() > state.config.mail.max_message_bytes {
        return Err(AppError::PayloadTooLarge);
    }

    let id = pipeline::ingest_raw(&state, &body).await?;
    Ok(Json(json!({ "id": id })))
}

fn compute_hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}
