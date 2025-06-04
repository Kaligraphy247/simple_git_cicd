use crate::{CICDConfig, ProjectConfig};
use tracing::{self, error, info};

// For signature verification
use hex::decode as hex_decode;
use hmac::{Hmac, Mac};
use sha2::Sha256;
type HmacSha256 = Hmac<Sha256>;

/// Helper function for verifying GitHub webhook signature
pub fn verify_github_signature(secret: &str, payload: &[u8], signature_header: &str) -> bool {
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
pub fn find_matching_project<'a>(
    config: &'a CICDConfig,
    repo_name: &str,
    branch: &str,
) -> Option<&'a ProjectConfig> {
    config
        .project
        .iter()
        .find(|proj| proj.name == repo_name && proj.branches.iter().any(|b| b == branch))
}

pub fn find_matching_project_owned(
    config: &CICDConfig,
    repo_name: &str,
    branch: &str,
) -> Option<ProjectConfig> {
    config
        .project
        .iter()
        .find(|proj| proj.name == repo_name && proj.branches.iter().any(|b| b == branch))
        .cloned()
}

/// Helper to run the pipeline: git checkout, git pull, then the user script within the right directory.
/// Returns combined stdout/stderr output or error.
pub async fn run_job_pipeline(
    branch: &str,
    repo_path: &str,
    run_script: &str,
) -> Result<String, String> {
    use tokio::process::Command;
    use tracing::{error, info};

    // 0. git fetch to update remote refs
    info!("Running (cwd = '{}'): git fetch", repo_path);
    let fetch = Command::new("git")
        .current_dir(repo_path)
        .arg("fetch")
        .output()
        .await
        .map_err(|e| {
            error!("git fetch failed to start: {}", e);
            format!("git fetch failed to start: {}", e)
        })?;
    if !fetch.status.success() {
        let msg = format!(
            "git fetch failed: {}",
            String::from_utf8_lossy(&fetch.stderr)
        );
        error!("{}", msg);
        return Err(msg);
    }
    info!(
        "git fetch output:\n{}",
        String::from_utf8_lossy(&fetch.stdout)
    );

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
