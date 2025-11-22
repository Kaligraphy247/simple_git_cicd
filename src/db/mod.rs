use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;
use tracing::info;

pub mod store;

use crate::error::CicdError;
pub use store::SqlJobStore;

/// Initialize the SQLite database connection pool and run migrations
pub async fn init_db(db_path: impl AsRef<Path>) -> Result<SqlitePool, CicdError> {
    let db_path = db_path.as_ref();
    let db_path_str = db_path.to_string_lossy();

    // Ensure the database file exists or create it
    if !db_path.exists() {
        info!("Database file not found at {}, creating...", db_path_str);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CicdError::DatabaseError(format!("Failed to create database directory: {}", e))
            })?;
        }
        std::fs::File::create(db_path).map_err(|e| {
            CicdError::DatabaseError(format!("Failed to create database file: {}", e))
        })?;
    }

    let db_url = format!("sqlite:{}", db_path_str);
    info!("Connecting to database at {}", db_url);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .map_err(|e| CicdError::ConfigError(format!("Failed to connect to database: {}", e)))?;

    info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| CicdError::ConfigError(format!("Failed to run migrations: {}", e)))?;

    info!("Database initialized successfully");
    Ok(pool)
}
