use crate::state::AppState;
use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post},
};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub mod addresses;
pub mod auth;
pub mod ingest;
pub mod messages;
pub mod send;
pub mod stream;

pub fn router(state: AppState) -> Router {
    let ingest_routes = Router::new()
        .route("/v1/ingest", post(ingest::ingest))
        .with_state(state.clone());

    let public_routes = Router::new()
        .route("/v1/config", get(public_config))
        .with_state(state.clone());

    let app_routes = Router::new()
        .route(
            "/v1/addresses",
            get(addresses::list).post(addresses::create),
        )
        .route("/v1/addresses/{local_part}", delete(addresses::delete))
        .route("/v1/messages", get(messages::list))
        .route(
            "/v1/messages/{id}",
            get(messages::get_one).delete(messages::delete_one),
        )
        .route("/v1/messages/{id}/read", post(messages::mark_read))
        .route("/v1/messages/{id}/raw", get(messages::raw_eml))
        .route(
            "/v1/messages/{id}/attachments/{aid}",
            get(messages::attachment),
        )
        .route("/v1/events", get(stream::stream))
        .route("/v1/send", post(send::send))
        .layer(from_fn_with_state(state.clone(), auth::require_bearer))
        .with_state(state.clone());

    Router::new()
        .merge(ingest_routes)
        .merge(public_routes)
        .merge(app_routes)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}

async fn public_config(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "domain": state.config.mail.domain,
        "catch_all": state.config.mail.catch_all,
        "max_message_bytes": state.config.mail.max_message_bytes,
    }))
}
