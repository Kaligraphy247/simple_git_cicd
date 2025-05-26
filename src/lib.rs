pub mod utils;

use serde::Deserialize;
use std::sync::Arc;
use std::collections::HashMap;
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
}

pub struct AppState {
    pub app_lock_state: Mutex<()>,
    pub config: CICDConfig,
}

pub type SharedState = Arc<AppState>;
