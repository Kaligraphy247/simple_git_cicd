use axum::{Router, routing};
use chrono::Utc;
use simple_git_cicd::api::{
    get_config, get_job, get_job_logs, get_jobs, get_projects, get_stats, handle_webhook,
    reload_config_endpoint, status, stream_jobs, stream_logs,
};
use simple_git_cicd::db::{SqlJobStore, init_db};
use simple_git_cicd::error::CicdError;
use simple_git_cicd::rate_limit::RateLimiter;
use simple_git_cicd::ui::serve_ui;
use simple_git_cicd::{AppState, CICDConfig};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::{Mutex, broadcast};
use tracing::info;
use tracing_subscriber::EnvFilter;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:8888";
const DEFAULT_CONFIG_PATH: &str = "cicd_config.toml";
const DEFAULT_DB_PATH: &str = "cicd_data.db";

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

    // Initialize tracing with environment filter
    // Use RUST_LOG env var to control log levels (e.g., RUST_LOG=debug or RUST_LOG=simple_git_cicd=trace)
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            EnvFilter::new("simple_git_cicd=debug,tower_http=debug")
        } else {
            EnvFilter::new("simple_git_cicd=info")
        }
    });

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let bind_address =
        std::env::var("BIND_ADDRESS").unwrap_or_else(|_| DEFAULT_BIND_ADDRESS.to_string());
    let config_path =
        std::env::var("CICD_CONFIG").unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());
    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());

    let config: CICDConfig = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    let pool = match init_db(&db_path).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Database initialization error: {}", e);
            std::process::exit(1);
        }
    };

    let job_store = SqlJobStore::new(pool);
    let start_time = Instant::now();
    let started_at = Utc::now();
    let (job_events, _) = broadcast::channel(100);
    let (log_chunks, _) = broadcast::channel(1000); // Higher capacity for streaming logs
    let rate_limiter = Arc::new(tokio::sync::Mutex::new(RateLimiter::new()));

    let state = Arc::new(AppState {
        job_execution_lock: Mutex::new(()),
        job_store,
        config: RwLock::new(config),
        config_path: PathBuf::from(config_path.clone()),
        start_time,
        started_at,
        rate_limiter,
        job_events,
        log_chunks,
    });

    let app = Router::new()
        // Webhook endpoint (kept at root for GitHub compatibility)
        .route("/webhook", routing::post(handle_webhook))
        // API endpoints
        .route("/api/status", routing::get(status))
        .route("/api/reload", routing::post(reload_config_endpoint))
        .route("/api/jobs", routing::get(get_jobs))
        .route("/api/jobs/{id}", routing::get(get_job))
        .route("/api/jobs/{id}/logs", routing::get(get_job_logs))
        .route("/api/projects", routing::get(get_projects))
        .route("/api/stats", routing::get(get_stats))
        .route("/api/config/current", routing::get(get_config))
        // SSE streams
        .route("/api/stream/jobs", routing::get(stream_jobs))
        .route("/api/stream/logs", routing::get(stream_logs))
        .with_state(state)
        // UI fallback - serves embedded static files
        .fallback(serve_ui);

    info!("Listening on {}", bind_address);
    info!("Using config at {:?}", config_path);
    let listener = tokio::net::TcpListener::bind(bind_address).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
