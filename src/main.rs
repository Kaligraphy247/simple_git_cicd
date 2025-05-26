#![allow(unused_imports)]
use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing,
};
use serde::Deserialize;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{self, debug, error, info, warn};

// For signature verification
use hex::decode as hex_decode;
use hmac::{Hmac, Mac};
use sha2::Sha256;
type HmacSha256 = Hmac<Sha256>;

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
    pub with_webhook_secret: Option<bool>,
    pub webhook_secret: Option<String>,
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
}

struct AppState {
    app_lock_state: Mutex<()>,
    config: CICDConfig,
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    // Load and parse TOML config
    let config_path =
        std::env::var("CICD_CONFIG").unwrap_or_else(|_| "cicd_config.toml".to_string());
    let config_str = fs::read_to_string(&config_path)
        .unwrap_or_else(|_| panic!("Failed to read config file: {}", config_path));
    let config: CICDConfig =
        toml::from_str(&config_str).unwrap_or_else(|e| panic!("Failed to parse config: {:?}", e));

    let state = Arc::new(AppState {
        app_lock_state: Mutex::new(()),
        config,
    });

    tracing_subscriber::fmt::init();
    let app = Router::new()
        .route("/", routing::get(root))
        .route("/webhook", routing::post(handle_webhook))
        .with_state(state);

    info!("Listening on port 0.0.0.0:8888");
    info!("Using config at {:?}", config_path);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8888").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "Hello, World!"
}

use axum::extract::State as AxumState;

/// Handles the GitHub webhook POST request.
async fn handle_webhook(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Only handle "push" events.
    let event_opt = headers.get("X-GitHub-Event").and_then(|v| v.to_str().ok());
    if event_opt != Some("push") {
        info!("Not push event; Received {:?} event", event_opt);
        return StatusCode::NO_CONTENT;
    }

    // Parse body as JSON and extract "ref" (branch) and repo name
    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            info!("Could not parse JSON body: {:?}", e);
            return StatusCode::BAD_REQUEST;
        }
    };

    let branch_ref = payload.get("ref").and_then(|r| r.as_str());
    debug!("{:#?}", &payload);
    let repo_name = payload
        .get("repository")
        .and_then(|r| r.get("name"))
        .and_then(|n| n.as_str());

    if branch_ref.is_none() || repo_name.is_none() {
        error!("No ref or repository.name field in push event payload");
        return StatusCode::BAD_REQUEST;
    }
    let branch_ref = branch_ref.unwrap();
    let branch_name = branch_ref.strip_prefix("refs/heads/").unwrap_or(branch_ref);
    let repo_name = repo_name.unwrap();

    // Find matching project config based on repo name and branch
    let maybe_project = find_matching_project(&state.config, repo_name, branch_name);

    if let Some(project) = maybe_project {
        // Per-project webhook signature validation if required
        if project.needs_webhook_secret() {
            // Validate there is a signature header and a valid secret
            let signature_opt = headers
                .get("X-Hub-Signature-256")
                .and_then(|v| v.to_str().ok());
            if signature_opt.is_none() {
                error!(
                    "Project '{}' requires webhook secret, but no signature header supplied.",
                    project.name
                );
                return StatusCode::UNAUTHORIZED;
            }
            if !project.has_valid_secret() {
                error!(
                    "Project '{}' requires webhook secret, but none was configured.",
                    project.name
                );
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
            let signature = signature_opt.unwrap();
            let secret = project.webhook_secret.as_ref().unwrap();
            let valid = verify_github_signature(secret, &body, signature);
            if !valid {
                error!(
                    "Signature verification failed for project '{}'!",
                    project.name
                );
                return StatusCode::UNAUTHORIZED;
            }
        }

        // Acquire the job lock. Only one job will run at a time.
        // Good for servers with low resources, don't bait
        // the OOM killer
        let _guard = state.app_lock_state.lock().await;
        info!(
            "Push event for project '{}' branch '{}'. Starting job pipeline.",
            repo_name, branch_name
        );

        // Run the job (git checkout, pull, then user script)
        match run_job_pipeline(branch_name, &project.repo_path, &project.run_script).await {
            Ok(_output) => {
                info!("Job completed successfully.");
                StatusCode::OK
            }
            Err(e) => {
                error!("Job failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    } else {
        warn!(
            "No matching project for repo '{}' and branch '{}', skipping.",
            repo_name, branch_name
        );
        StatusCode::NO_CONTENT
    }
}

/// Helper function for verifying GitHub webhook signature
fn verify_github_signature(secret: &str, payload: &[u8], signature_header: &str) -> bool {
    // Expected format: "sha256=..."
    let expected_prefix = "sha256=";
    if !signature_header.starts_with(expected_prefix) {
        return false;
    }

    // signature from git
    let git_signature = &signature_header[expected_prefix.len()..];

    // Compute HMAC SHA256
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(payload);
    let my_sig = mac.finalize().into_bytes();

    // GitHub provides the signature as hex
    match hex_decode(git_signature) {
        Ok(git_signature_bytes) => {
            // Constant-time comparison
            my_sig.as_slice() == git_signature_bytes.as_slice()
        }
        Err(_) => {
            error!("Signature verification failed");
            false
        }
    }
}

/// Finds the first project config matching both repository name and branch.
/// Returns None if there's no suitable match.
fn find_matching_project<'a>(
    config: &'a CICDConfig,
    repo_name: &str,
    branch: &str,
) -> Option<&'a ProjectConfig> {
    config
        .project
        .iter()
        .find(|proj| proj.name == repo_name && proj.branches.iter().any(|b| b == branch))
}

/// Helper to run the pipeline: git checkout, git pull, then the user script within the right directory.
/// Returns combined stdout/stderr output or error.
async fn run_job_pipeline(
    branch: &str,
    repo_path: &str,
    run_script: &str,
) -> Result<String, String> {
    use tokio::process::Command;
    use tracing::{error, info};

    // 1. git switch to branch
    info!("Running (cwd = '{}'): git switch {}", repo_path, branch);
    let checkout = Command::new("git")
        .current_dir(repo_path)
        .arg("switch")
        .arg(branch)
        .output()
        .await
        .map_err(|e| {
            error!("git switch failed to start: {}", e);
            format!("git switch failed to start: {}", e)
        })?;
    if !checkout.status.success() {
        let msg = format!(
            "git switch failed: {}",
            String::from_utf8_lossy(&checkout.stderr)
        );
        error!("{}", msg);
        return Err(msg);
    }
    info!(
        "git switch output:\n{}",
        String::from_utf8_lossy(&checkout.stdout)
    );

    // 2. git pull
    info!("Running (cwd = '{}'): git pull", repo_path);
    let pull = Command::new("git")
        .current_dir(repo_path)
        .arg("pull")
        .output()
        .await
        .map_err(|e| {
            error!("git pull failed to start: {}", e);
            format!("git pull failed to start: {}", e)
        })?;
    if !pull.status.success() {
        let msg = format!("git pull failed: {}", String::from_utf8_lossy(&pull.stderr));
        error!("{}", msg);
        return Err(msg);
    }
    info!(
        "git pull output:\n{}",
        String::from_utf8_lossy(&pull.stdout)
    );

    // 3. Run the user script (split by whitespace for command + args)
    let mut parts = run_script.split_whitespace();
    // Extract the script to run from the 'run_script' string.
    // 'parts' is an iterator over the whitespace-separated words in 'run_script'.
    // The first element is expected to be the command (e.g., "cargo" in "cargo build").
    // Return an error if 'run_script' is empty, logging for diagnostics.
    let script = parts.next().ok_or_else(|| {
        let msg = "RUN_SCRIPT is empty".to_string();
        error!("{}", msg);
        msg
    })?;
    let args: Vec<&str> = parts.collect();

    use std::fmt::Write as _;
    let mut cmd_str = String::new();
    write!(
        &mut cmd_str,
        "{}{}",
        script,
        if !args.is_empty() { " " } else { "" }
    )
    .ok();
    for (idx, arg) in args.iter().enumerate() {
        write!(
            &mut cmd_str,
            "{}{}",
            arg,
            if idx + 1 != args.len() { " " } else { "" }
        )
        .ok();
    }

    // Log the REAL user script command
    let mut script_cmd = String::from(script);
    for arg in &args {
        script_cmd.push(' ');
        script_cmd.push_str(arg);
    }
    info!("Running (cwd = '{}'): {}", repo_path, script_cmd);
    let script_output = Command::new(script)
        .current_dir(repo_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| {
            error!("run_script failed to start: {}", e);
            format!("run_script failed to start: {}", e)
        })?;

    if script_output.status.success() {
        info!(
            "run_script output:\n{}",
            String::from_utf8_lossy(&script_output.stdout)
        );
        Ok(String::from_utf8_lossy(&script_output.stdout).to_string())
    } else {
        let msg = format!(
            "run_script failed:\n{}",
            String::from_utf8_lossy(&script_output.stderr)
        );
        error!("{}", msg);
        Err(msg)
    }
}
