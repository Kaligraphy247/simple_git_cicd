//! Config API endpoints

use axum::{extract::State as AxumState, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use serde_json::json;
use tokio::fs;
use tracing::{error, info};

use crate::{reload_config, validate_config, SharedState};

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
            config_toml: redact_secrets(&content),
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

/// Redact `webhook_secret` values from a raw TOML string while leaving
/// everything else (formatting, comments, key order, whitespace) byte-identical.
///
/// This is deliberately a line-oriented transformation rather than a
/// parse-and-reserialize round trip through `toml`, since re-serializing
/// would strip comments and could reorder keys in the config viewer.
///
/// Only the exact key `webhook_secret` is matched (allowing leading
/// whitespace and optional whitespace around `=`); `with_webhook_secret`
/// (a boolean toggle) is a different key and is left untouched.
fn redact_secrets(config_toml: &str) -> String {
    const REDACTED: &str = "\"***REDACTED***\"";

    // `split_inclusive` keeps each line's terminator attached, so the trailing
    // newline and any CRLF endings survive the round trip untouched.
    config_toml
        .split_inclusive('\n')
        .map(|line| {
            let (content, ending) = match line.strip_suffix('\n') {
                Some(rest) => match rest.strip_suffix('\r') {
                    Some(rest) => (rest, "\r\n"),
                    None => (rest, "\n"),
                },
                None => (line, ""),
            };

            let trimmed = content.trim_start();
            let indent = &content[..content.len() - trimmed.len()];

            if trimmed
                .strip_prefix("webhook_secret")
                .is_some_and(|rest| rest.trim_start().starts_with('='))
            {
                return format!("{indent}webhook_secret = {REDACTED}{ending}");
            }

            line.to_string()
        })
        .collect()
}

/// POST /api/reload - Reload configuration from disk
/// Waits for current job to finish before applying the new config
pub async fn reload_config_endpoint(AxumState(state): AxumState<SharedState>) -> impl IntoResponse {
    // Wait for current job to finish before reloading
    let _guard = state.job_execution_lock.lock().await;

    match reload_config(&state.config_path).await {
        Ok(new_config) => {
            // Validate the new config before applying it
            if let Err(e) = validate_config(&new_config) {
                error!("Config validation failed during reload: {}", e);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "status": "error",
                        "message": format!("Configuration validation failed: {}", e)
                    })),
                )
                    .into_response();
            }

            let mut config = state.config.write().unwrap();
            *config = new_config;
            info!(
                "Configuration validated successfully from {:?} and reloaded",
                state.config_path
            );
            Json(json!({
                "status": "success",
                "message": "Configuration validated and reloaded successfully"
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

#[cfg(test)]
mod tests {
    use super::redact_secrets;

    #[test]
    fn redacts_webhook_secret_line() {
        let input = r#"webhook_secret = "supersecret""#;
        assert_eq!(
            redact_secrets(input),
            r#"webhook_secret = "***REDACTED***""#
        );
    }

    #[test]
    fn leaves_with_webhook_secret_untouched() {
        let input = "with_webhook_secret = true";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn redacts_unusual_spacing() {
        assert_eq!(
            redact_secrets(r#"webhook_secret="x""#),
            r#"webhook_secret = "***REDACTED***""#
        );
        assert_eq!(
            redact_secrets(r#"  webhook_secret   =   "x""#),
            r#"  webhook_secret = "***REDACTED***""#
        );
    }

    #[test]
    fn leaves_comments_and_other_lines_unchanged() {
        let input = "# this is a comment\nname = \"repo\"\nbranches = [\"main\"]";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn preserves_trailing_newline_and_crlf() {
        let input = "name = \"repo\"\nwebhook_secret = \"s\"\n";
        assert_eq!(
            redact_secrets(input),
            "name = \"repo\"\nwebhook_secret = \"***REDACTED***\"\n"
        );

        let crlf = "name = \"repo\"\r\nwebhook_secret = \"s\"\r\n";
        assert_eq!(
            redact_secrets(crlf),
            "name = \"repo\"\r\nwebhook_secret = \"***REDACTED***\"\r\n"
        );
    }

    #[test]
    fn redacts_multiple_projects() {
        let input = "\
[[project]]
name = \"repo-a\"
webhook_secret = \"secret-a\"
with_webhook_secret = true

[[project]]
name = \"repo-b\"
webhook_secret = \"secret-b\"
with_webhook_secret = false";

        let expected = "\
[[project]]
name = \"repo-a\"
webhook_secret = \"***REDACTED***\"
with_webhook_secret = true

[[project]]
name = \"repo-b\"
webhook_secret = \"***REDACTED***\"
with_webhook_secret = false";

        assert_eq!(redact_secrets(input), expected);
    }
}
