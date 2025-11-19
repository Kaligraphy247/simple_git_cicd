use std::io;

/// Custom error type for simple_git_cicd operations
#[derive(Debug, thiserror::Error)]
pub enum CicdError {
    #[error("Git operation failed: {operation}\n{message}")]
    GitOperationFailed { operation: String, message: String },

    #[error("Script execution failed: {0}")]
    ScriptExecutionFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Webhook validation failed: {0}")]
    WebhookValidationFailed(String),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("TOML parsing error: {0}")]
    TomlParseError(#[from] toml::de::Error),
}

/// Helper type for Results that use CicdError
pub type Result<T> = std::result::Result<T, CicdError>;
