//! Config API endpoints

use axum::{Json, extract::State as AxumState, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use serde_json::json;
use tokio::fs;

use crate::SharedState;

/// Response for config content
#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub config_toml: String,
    pub path: String,
}

/// GET /api/config/current - Get current configuration file content
pub async fn get_config(AxumState(state): AxumState<SharedState>) -> impl IntoResponse {
    let path = &state.config_path;

    match fs::read_to_string(path).await {
        Ok(content) => Json(ConfigResponse {
            config_toml: content,
            path: path.to_string_lossy().into_owned(),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("Failed to read config file: {}", e)
            })),
        )
            .into_response(),
    }
}
