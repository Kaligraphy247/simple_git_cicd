pub mod api;
pub mod db;
pub mod error;
pub mod job;
pub mod rate_limit;
pub mod ui;
pub mod utils;
pub mod webhook;

use api::stream::{JobEvent, LogChunkEvent};
use chrono::{DateTime, Utc};
use db::SqlJobStore;
use rate_limit::RateLimiter;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::{Mutex, broadcast};

#[derive(Debug, Deserialize, Clone)]
pub struct CICDConfig {
    pub project: Vec<ProjectConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProjectConfig {
    pub name: String,
    pub repo_path: String,
    pub branches: Vec<String>,
    pub run_script: String,
    pub branch_scripts: Option<HashMap<String, String>>,
    pub with_webhook_secret: Option<bool>,
    pub webhook_secret: Option<String>,

    // ?
    pub reset_to_remote: Option<bool>,

    // lifecycle hooks
    pub pre_script: Option<String>,
    pub post_script: Option<String>,
    pub post_success_script: Option<String>,
    pub post_failure_script: Option<String>,
    pub post_always_script: Option<String>,

    // rate limiting
    pub rate_limit_requests: Option<usize>,
    pub rate_limit_window_seconds: Option<u64>,
}

impl ProjectConfig {
    /// Returns true if webhook secret validation should be enforced.
    pub fn needs_webhook_secret(&self) -> bool {
        self.with_webhook_secret.unwrap_or(false)
    }

    /// Returns true if a valid (non-empty) webhook_secret is set.
    pub fn has_valid_secret(&self) -> bool {
        self.webhook_secret
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    /// Returns the script to run for a specific branch.
    /// If `branch_scripts` contains the branch, returns that script,
    /// otherwise returns the general `run_script`.
    pub fn get_run_script_for_branch(&self, branch: &str) -> &str {
        self.branch_scripts
            .as_ref()
            .and_then(|scripts| scripts.get(branch))
            .map(|s| s.as_str())
            .unwrap_or(&self.run_script)
    }

    /// Returns the maximum number of requests allowed for rate limiting.
    /// Defaults to 60 if `rate_limit_requests` is not set.
    pub fn get_rate_limit(&self) -> usize {
        self.rate_limit_requests.unwrap_or(60)
    }

    /// Returns the time window in seconds for rate limiting.
    /// Defaults to 60 seconds if `rate_limit_window_seconds` is not set.
    pub fn get_rate_limit_window(&self) -> u64 {
        self.rate_limit_window_seconds.unwrap_or(60)
    }

    /// Returns true if git should reset to remote (default: true for CI/CD)
    pub fn should_reset_to_remote(&self) -> bool {
        self.reset_to_remote.unwrap_or(true)
    }
}

pub struct AppState {
    pub job_execution_lock: Mutex<()>,
    pub job_store: SqlJobStore,
    pub config: RwLock<CICDConfig>,
    pub config_path: PathBuf,
    pub start_time: Instant,
    pub started_at: DateTime<Utc>,
    pub rate_limiter: Arc<tokio::sync::Mutex<RateLimiter>>,
    pub job_events: broadcast::Sender<JobEvent>,
    pub log_chunks: broadcast::Sender<LogChunkEvent>,
}

/// Reload configuration from disk
pub async fn reload_config(config_path: &PathBuf) -> Result<CICDConfig, error::CicdError> {
    use std::fs;

    let config_str = fs::read_to_string(config_path)
        .map_err(|e| error::CicdError::ConfigError(format!("Failed to read config file: {}", e)))?;

    // Use toml crate to parse the config
    let new_config: CICDConfig = toml::from_str(&config_str)
        .map_err(|e| error::CicdError::ConfigError(format!("Failed to parse config: {}", e)))?;

    Ok(new_config)
}

// Shared application state wrapped in an Arc for thread-safe shared ownership
pub type SharedState = Arc<AppState>;
