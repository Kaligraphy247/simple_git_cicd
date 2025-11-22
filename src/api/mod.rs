//! API module for all HTTP handlers
//!
//! Contains both core endpoints and new REST API endpoints for the Web UI

pub mod config;
pub mod jobs;
pub mod projects;
pub mod stats;
pub mod stream;
pub mod webhook;

// Re-export handlers
pub use config::{get_config, reload_config_endpoint};
pub use jobs::{get_job, get_job_logs, get_jobs};
pub use projects::get_projects;
pub use stats::{get_stats, status};
pub use stream::{LogChunkEvent, stream_jobs, stream_logs};
pub use webhook::handle_webhook;
