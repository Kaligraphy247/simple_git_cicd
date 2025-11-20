//! API module for all HTTP handlers
//!
//! Contains both legacy endpoints and new REST API endpoints for the Web UI

pub mod handlers;
pub mod jobs;
pub mod projects;
pub mod stats;

// Re-export core handlers
pub use handlers::{root, status, handle_webhook, reload_config_endpoint};

// Re-export API handlers
pub use jobs::{get_jobs, get_job, get_job_logs};
pub use projects::get_projects;
pub use stats::get_stats;
