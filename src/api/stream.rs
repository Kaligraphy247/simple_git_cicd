//! SSE streaming endpoints for real-time job updates

use axum::{
    extract::State as AxumState,
    response::sse::{Event, Sse},
};
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::SharedState;

/// Job event for SSE broadcasting
#[derive(Debug, Clone, serde::Serialize)]
pub struct JobEvent {
    pub event_type: String, // created, running, success, failed
    pub job_id: String,
    pub project_name: String,
    pub branch: String,
    pub timestamp: String,
}

/// GET /api/stream/jobs - SSE stream of job status changes
pub async fn stream_jobs(
    AxumState(state): AxumState<SharedState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.job_events.subscribe();
    let stream = BroadcastStream::new(rx);

    let event_stream = stream.filter_map(|result| {
        match result {
            Ok(event) => {
                let data = serde_json::to_string(&event).unwrap_or_default();
                Some(Ok(Event::default().event(&event.event_type).data(data)))
            }
            Err(_) => None, // Skip lagged messages
        }
    });

    Sse::new(event_stream)
}
