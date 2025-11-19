mod handlers;

use axum::{Router, routing};
use chrono::Utc;
use handlers::{get_job, handle_webhook, root, status};
use simple_git_cicd::error::CicdError;
use simple_git_cicd::job::JobStore;
use simple_git_cicd::{AppState, CICDConfig};
use std::fs;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{self, info};

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:8888";
const DEFAULT_CONFIG_PATH: &str = "cicd_config.toml";
const DEFAULT_MAX_JOBS: usize = 24;

/// Load and parse the configuration file
fn load_config(path: &str) -> Result<CICDConfig, CicdError> {
    let config_str = fs::read_to_string(path).map_err(|e| {
        CicdError::ConfigError(format!("Failed to read config file '{}': {}", path, e))
    })?;

    let config: CICDConfig = toml::from_str(&config_str).map_err(|e| {
        CicdError::ConfigError(format!("Failed to parse config file '{}': {}", path, e))
    })?;

    Ok(config)
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let bind_address =
        std::env::var("BIND_ADDRESS").unwrap_or_else(|_| DEFAULT_BIND_ADDRESS.to_string());
    let config_path =
        std::env::var("CICD_CONFIG").unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());

    let config: CICDConfig = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    let job_store = Arc::new(Mutex::new(JobStore::new(DEFAULT_MAX_JOBS)));
    let start_time = Instant::now();
    let started_at = Utc::now();

    let state = Arc::new(AppState {
        job_execution_lock: Mutex::new(()),
        job_store,
        config,
        start_time,
        started_at,
    });

    tracing_subscriber::fmt::init();
    let app = Router::new()
        .route("/", routing::get(root))
        .route("/webhook", routing::post(handle_webhook))
        .route("/status", routing::get(status))
        .route("/job/:id", routing::get(get_job))
        .with_state(state);

    info!("Listening on {}", bind_address);
    info!("Using config at {:?}", config_path);
    let listener = tokio::net::TcpListener::bind(bind_address).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
