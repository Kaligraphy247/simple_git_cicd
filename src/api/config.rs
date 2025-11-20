//! Config API endpoints

use axum::{Json, extract::State as AxumState, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use serde_json::json;
use tokio::fs;
use tracing::{error, info};

use crate::{SharedState, reload_config};

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

/// POST /api/reload - Reload configuration from disk
/// Waits for current job to finish before applying the new config
pub async fn reload_config_endpoint(AxumState(state): AxumState<SharedState>) -> impl IntoResponse {
    // Wait for current job to finish before reloading
    let _guard = state.job_execution_lock.lock().await;

    match reload_config(&state.config_path).await {
        Ok(new_config) => {
            let mut config = state.config.write().unwrap();
            *config = new_config;
            info!(
                "Configuration reloaded successfully from {:?}",
                state.config_path
            );
            Json(json!({
                "status": "success",
                "message": "Configuration reloaded successfully"
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to reload config: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": e.to_string()
                })),
            )
                .into_response()
        }
    }
}
