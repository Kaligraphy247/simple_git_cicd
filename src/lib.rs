pub mod error;
pub mod job;
pub mod utils;
pub mod webhook;

use chrono::{DateTime, Utc};
use job::JobStore;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::Mutex;

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

    // Coming soon: Advanced features
    pub reset_to_remote: Option<bool>,
    pub pre_script: Option<String>,
    pub post_script: Option<String>,
    pub post_success_script: Option<String>,
    pub post_failure_script: Option<String>,
    pub post_always_script: Option<String>,
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
        if let Some(scripts) = &self.branch_scripts {
            if let Some(custom_script) = scripts.get(branch) {
                return custom_script;
            }
        }
        &self.run_script
    }

    /// Returns true if git should reset to remote (default: true for CI/CD)
    pub fn should_reset_to_remote(&self) -> bool {
        self.reset_to_remote.unwrap_or(true)
    }
}

pub struct AppState {
    pub job_execution_lock: Mutex<()>,
    pub job_store: Arc<Mutex<JobStore>>,
    pub config: RwLock<CICDConfig>,
    pub config_path: PathBuf,
    pub start_time: Instant,
    pub started_at: DateTime<Utc>,
}

pub type SharedState = Arc<AppState>;

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
