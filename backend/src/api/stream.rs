use crate::state::AppState;
use axum::{
    extract::State,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use futures::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.events.subscribe();
    let s = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(evt) => serde_json::to_string(&evt)
            .ok()
            .map(|s| Ok(SseEvent::default().data(s))),
        Err(_) => None,
    });
    Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
