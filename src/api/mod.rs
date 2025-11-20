//! API module for all HTTP handlers
//!
//! Contains both core endpoints and new REST API endpoints for the Web UI

pub mod config;
pub mod handlers;
pub mod jobs;
pub mod projects;
pub mod stats;
pub mod stream;

// Re-export core handlers
pub use handlers::{handle_webhook, reload_config_endpoint, root, status};

// Re-export API handlers
pub use config::get_config;
pub use jobs::{get_job, get_job_logs, get_jobs};
pub use projects::get_projects;
pub use stats::get_stats;
pub use stream::stream_jobs;
