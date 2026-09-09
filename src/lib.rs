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
use tracing::info;

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
    pub branch_repo_paths: Option<HashMap<String, String>>,
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
        if let Some(scripts) = &self.branch_scripts
            && let Some(script) = scripts.get(branch)
        {
            info!(
                project = %self.name,
                branch = %branch,
                script = %script,
                "Using branch-specific script"
            );
            return script;
        }
        info!(
            project = %self.name,
            branch = %branch,
            script = %self.run_script,
            "Using default script (no branch-specific override found)"
        );
        &self.run_script
    }

    /// Returns the repo path to use for a specific branch.
    /// If `branch_repo_paths` contains the branch, returns that path,
    /// otherwise returns the general `repo_path`.
    pub fn get_repo_path_for_branch(&self, branch: &str) -> &str {
        if let Some(paths) = &self.branch_repo_paths
            && let Some(path) = paths.get(branch)
        {
            info!(
                project = %self.name,
                branch = %branch,
                repo_path = %path,
                "Using branch-specific repo path"
            );
            return path;
        }
        info!(
            project = %self.name,
            branch = %branch,
            repo_path = %self.repo_path,
            "Using default repo path (no branch-specific override found)"
        );
        &self.repo_path
    }

    /// Returns the maximum number of requests allowed for rate limiting.
    /// Defaults to 10 if `rate_limit_requests` is not set.
    pub fn get_rate_limit(&self) -> usize {
        self.rate_limit_requests.unwrap_or(10)
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

/// Helper function to expand path with ~ and resolve relative paths
fn expand_path(path: &str) -> PathBuf {
    use std::env;

    let path = if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            PathBuf::from(home).join(stripped)
        } else {
            PathBuf::from(path)
        }
    } else {
        PathBuf::from(path)
    };

    // Convert to absolute path if relative
    if path.is_relative() {
        if let Ok(current_dir) = env::current_dir() {
            current_dir.join(path)
        } else {
            path
        }
    } else {
        path
    }
}

/// Helper function to check if a string looks like a path
/// Paths typically contain '/' or '\' or start with './' or '../' or '~/'
fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.contains('\\') || s.starts_with("~/")
}

/// Validate configuration
pub fn validate_config(config: &CICDConfig) -> Result<(), error::CicdError> {
    if config.project.is_empty() {
        return Err(error::CicdError::ConfigError(
            "Configuration must contain at least one project".to_string(),
        ));
    }

    for (idx, project) in config.project.iter().enumerate() {
        let project_prefix = format!("Project '{}' (index {})", project.name, idx);

        // Validate project name is not empty
        if project.name.is_empty() {
            return Err(error::CicdError::ConfigError(format!(
                "{}: Project name cannot be empty",
                project_prefix
            )));
        }

        // Validate branches list is not empty
        if project.branches.is_empty() {
            return Err(error::CicdError::ConfigError(format!(
                "{}: Must specify at least one branch",
                project_prefix
            )));
        }

        // Validate default repo_path exists (always validate as path, expand ~)
        let repo_path = expand_path(&project.repo_path);
        if !repo_path.exists() {
            return Err(error::CicdError::ConfigError(format!(
                "{}: Default repo_path '{}' (resolved to '{}') does not exist",
                project_prefix,
                project.repo_path,
                repo_path.display()
            )));
        }
        if !repo_path.is_dir() {
            return Err(error::CicdError::ConfigError(format!(
                "{}: Default repo_path '{}' (resolved to '{}') is not a directory",
                project_prefix,
                project.repo_path,
                repo_path.display()
            )));
        }

        // Validate branch-specific repo paths
        if let Some(branch_paths) = &project.branch_repo_paths {
            for (branch, path) in branch_paths {
                // Check branch exists in branches list
                if !project.branches.contains(branch) {
                    return Err(error::CicdError::ConfigError(format!(
                        "{}: Branch '{}' in branch_repo_paths is not in branches list",
                        project_prefix, branch
                    )));
                }

                // Check path exists (expand ~ and relative paths)
                let branch_path = expand_path(path);
                if !branch_path.exists() {
                    return Err(error::CicdError::ConfigError(format!(
                        "{}: Branch-specific repo_path '{}' (resolved to '{}') for branch '{}' does not exist",
                        project_prefix,
                        path,
                        branch_path.display(),
                        branch
                    )));
                }
                if !branch_path.is_dir() {
                    return Err(error::CicdError::ConfigError(format!(
                        "{}: Branch-specific repo_path '{}' (resolved to '{}') for branch '{}' is not a directory",
                        project_prefix,
                        path,
                        branch_path.display(),
                        branch
                    )));
                }
            }
        }

        // Validate default run_script (if it looks like a path, check it exists; otherwise allow inline commands)
        if project.run_script.trim().is_empty() {
            return Err(error::CicdError::ConfigError(format!(
                "{}: Default run_script cannot be empty",
                project_prefix
            )));
        }

        if looks_like_path(&project.run_script) {
            let script_path = expand_path(&project.run_script);
            if !script_path.exists() {
                return Err(error::CicdError::ConfigError(format!(
                    "{}: Default run_script '{}' (resolved to '{}') does not exist",
                    project_prefix,
                    project.run_script,
                    script_path.display()
                )));
            }
        }

        // Validate branch-specific scripts
        if let Some(branch_scripts) = &project.branch_scripts {
            for (branch, script) in branch_scripts {
                // Check branch exists in branches list
                if !project.branches.contains(branch) {
                    return Err(error::CicdError::ConfigError(format!(
                        "{}: Branch '{}' in branch_scripts is not in branches list",
                        project_prefix, branch
                    )));
                }

                // Check script (if path, validate; otherwise allow inline command)
                if script.trim().is_empty() {
                    return Err(error::CicdError::ConfigError(format!(
                        "{}: Branch-specific script for branch '{}' cannot be empty",
                        project_prefix, branch
                    )));
                }

                if looks_like_path(script) {
                    let script_path = expand_path(script);
                    if !script_path.exists() {
                        return Err(error::CicdError::ConfigError(format!(
                            "{}: Branch-specific script '{}' (resolved to '{}') for branch '{}' does not exist",
                            project_prefix,
                            script,
                            script_path.display(),
                            branch
                        )));
                    }
                }
            }
        }

        // Validate lifecycle hook scripts if they exist
        let hooks = [
            ("pre_script", &project.pre_script),
            ("post_script", &project.post_script),
            ("post_success_script", &project.post_success_script),
            ("post_failure_script", &project.post_failure_script),
            ("post_always_script", &project.post_always_script),
        ];

        for (hook_name, hook_script) in &hooks {
            if let Some(script) = hook_script {
                if script.trim().is_empty() {
                    return Err(error::CicdError::ConfigError(format!(
                        "{}: {} cannot be empty",
                        project_prefix, hook_name
                    )));
                }

                // Only validate as path if it looks like a path
                if looks_like_path(script) {
                    let script_path = expand_path(script);
                    if !script_path.exists() {
                        return Err(error::CicdError::ConfigError(format!(
                            "{}: {} '{}' (resolved to '{}') does not exist",
                            project_prefix,
                            hook_name,
                            script,
                            script_path.display()
                        )));
                    }
                }
            }
        }

        // Validate webhook secret configuration
        if project.needs_webhook_secret() && !project.has_valid_secret() {
            return Err(error::CicdError::ConfigError(format!(
                "{}: with_webhook_secret is true but webhook_secret is not set or is empty",
                project_prefix
            )));
        }
    }

    Ok(())
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
