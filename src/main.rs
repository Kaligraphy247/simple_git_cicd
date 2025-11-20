mod handlers;

use axum::{Router, routing};
use chrono::Utc;
use handlers::{get_job, handle_webhook, reload_config_endpoint, root, status};
use simple_git_cicd::db::{SqlJobStore, init_db};
use simple_git_cicd::error::CicdError;
use simple_git_cicd::{AppState, CICDConfig};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{self, info};

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
    tracing_subscriber::fmt::init();

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

    let state = Arc::new(AppState {
        job_execution_lock: Mutex::new(()),
        job_store,
        config: RwLock::new(config),
        config_path: PathBuf::from(config_path.clone()),
        start_time,
        started_at,
    });

    let app = Router::new()
        .route("/", routing::get(root))
        .route("/webhook", routing::post(handle_webhook))
        .route("/status", routing::get(status))
        .route("/job/{id}", routing::get(get_job))
        .route("/reload", routing::post(reload_config_endpoint))
        .with_state(state);

    info!("Listening on {}", bind_address);
    info!("Using config at {:?}", config_path);
    let listener = tokio::net::TcpListener::bind(bind_address).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
