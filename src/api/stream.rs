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

/// Log chunk event for real-time log streaming
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogChunkEvent {
    pub job_id: String,
    pub step_type: String, // git_fetch, main_script, etc.
    pub chunk: String,
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

/// GET /api/stream/logs - SSE stream of real-time log chunks
pub async fn stream_logs(
    AxumState(state): AxumState<SharedState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.log_chunks.subscribe();
    let stream = BroadcastStream::new(rx);

    let event_stream = stream.filter_map(|result| {
        match result {
            Ok(chunk) => {
                let data = serde_json::to_string(&chunk).unwrap_or_default();
                Some(Ok(Event::default().event("log_chunk").data(data)))
            }
            Err(_) => None, // Skip lagged messages
        }
    });

    Sse::new(event_stream)
}
