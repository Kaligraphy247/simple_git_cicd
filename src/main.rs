mod handlers;

use axum::{Router, routing};
use handlers::{handle_webhook, root};
use simple_git_cicd::{AppState, CICDConfig};
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{self, info};

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let bind_address =
        std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8888".to_string());
    let config_path =
        std::env::var("CICD_CONFIG").unwrap_or_else(|_| "cicd_config.toml".to_string());
    let config_str = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read config file '{}': {}", config_path, e);
            std::process::exit(1);
        }
    };
    let config: CICDConfig = match toml::from_str(&config_str) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to parse config '{}': {:?}", config_path, e);
            std::process::exit(1);
        }
    };

    let state = Arc::new(AppState {
        app_lock_state: Mutex::new(()),
        config,
    });

    tracing_subscriber::fmt::init();
    let app = Router::new()
        .route("/", routing::get(root))
        .route("/webhook", routing::post(handle_webhook))
        .with_state(state);

    info!("Listening on {}", bind_address);
    info!("Using config at {:?}", config_path);
    let listener = tokio::net::TcpListener::bind(bind_address).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
