use crate::error::{CicdError, Result};
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

    // Extract signature from header
    let provided_signature = &signature_header[expected_prefix.len()..];

    // Compute HMAC SHA256
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(payload);
    let computed_signature = mac.finalize().into_bytes();

    // GitHub provides the signature as hex
    match hex_decode(provided_signature) {
        Ok(provided_signature_bytes) => {
            // Constant-time comparison
            computed_signature.as_slice() == provided_signature_bytes.as_slice()
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

/// Reset the repository to match the remote branch exactly (hard reset)
async fn git_reset_hard(repo_path: &str, target: &str) -> Result<()> {
    use tokio::process::Command;

    info!(
        "Running (cwd = '{}'): git reset --hard {}",
        repo_path, target
    );
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["reset", "--hard", target])
        .output()
        .await
        .map_err(|e| {
            error!("git reset --hard failed to start: {}", e);
            CicdError::GitOperationFailed {
                operation: "git reset --hard".to_string(),
                message: format!("Failed to start git process: {}", e),
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("git reset --hard failed: {}", stderr);
        return Err(CicdError::GitOperationFailed {
            operation: format!("git reset --hard {}", target),
            message: format!("{}. Ensure the target '{}' exists.", stderr.trim(), target),
        });
    }

    info!(
        "git reset --hard output:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(())
}

/// Helper to run the pipeline: git fetch, then reset/switch+pull, then run script
/// Returns combined stdout/stderr output or error.
pub async fn run_job_pipeline(
    branch: &str,
    repo_path: &str,
    run_script: &str,
    reset_to_remote: bool,
) -> Result<String> {
    use tokio::process::Command;
    use tracing::{error, info};

    // 1. git fetch to update remote refs
    info!("Running (cwd = '{}'): git fetch", repo_path);
    let fetch = Command::new("git")
        .current_dir(repo_path)
        .arg("fetch")
        .output()
        .await
        .map_err(|e| {
            error!("git fetch failed to start: {}", e);
            CicdError::GitOperationFailed {
                operation: "git fetch".to_string(),
                message: format!(
                    "Failed to start git process: {}. Ensure git is installed and accessible.",
                    e
                ),
            }
        })?;
    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        error!("git fetch failed: {}", stderr);
        return Err(CicdError::GitOperationFailed {
            operation: "git fetch".to_string(),
            message: format!(
                "{}. Check network connectivity and repository access.",
                stderr.trim()
            ),
        });
    }
    info!(
        "git fetch output:\n{}",
        String::from_utf8_lossy(&fetch.stdout)
    );

    // 2. Reset to remote or switch+pull
    if reset_to_remote {
        // CI/CD mode: Hard reset to match remote exactly (handles modified files)
        info!("Resetting to remote state (reset_to_remote=true)");
        git_reset_hard(repo_path, &format!("origin/{}", branch)).await?;
    } else {
        // Debug mode: Normal switch + pull
        info!("Using switch + pull mode (reset_to_remote=false)");

        // 2a. git switch to branch
        info!("Running (cwd = '{}'): git switch {}", repo_path, branch);
        let checkout = Command::new("git")
            .current_dir(repo_path)
            .arg("switch")
            .arg(branch)
            .output()
            .await
            .map_err(|e| {
                error!("git switch failed to start: {}", e);
                CicdError::GitOperationFailed {
                    operation: "git switch".to_string(),
                    message: format!("Failed to start git process: {}", e),
                }
            })?;
        if !checkout.status.success() {
            let stderr = String::from_utf8_lossy(&checkout.stderr);
            error!("git switch failed: {}", stderr);
            return Err(CicdError::GitOperationFailed {
                operation: format!("git switch {}", branch),
                message: format!(
                    "{}. Ensure branch '{}' exists remotely.",
                    stderr.trim(),
                    branch
                ),
            });
        }
        info!(
            "git switch output:\n{}",
            String::from_utf8_lossy(&checkout.stdout)
        );

        // 2b. git pull
        info!("Running (cwd = '{}'): git pull", repo_path);
        let pull = Command::new("git")
            .current_dir(repo_path)
            .arg("pull")
            .output()
            .await
            .map_err(|e| {
                error!("git pull failed to start: {}", e);
                CicdError::GitOperationFailed {
                    operation: "git pull".to_string(),
                    message: format!("Failed to start git process: {}", e),
                }
            })?;
        if !pull.status.success() {
            let stderr = String::from_utf8_lossy(&pull.stderr);
            error!("git pull failed: {}", stderr);
            return Err(CicdError::GitOperationFailed {
                operation: "git pull".to_string(),
                message: format!(
                    "{}. Ensure there are no local changes or merge conflicts.",
                    stderr.trim()
                ),
            });
        }
        info!(
            "git pull output:\n{}",
            String::from_utf8_lossy(&pull.stdout)
        );
    }

    // 3. Run the user script (split by whitespace for command + args)
    let mut parts = run_script.split_whitespace();
    // Extract the command to run from the 'run_script' string.
    // 'parts' is an iterator over the whitespace-separated words in 'run_script'.
    // The first element is expected to be the command (e.g., "cargo" in "cargo build").
    // Return an error if 'run_script' is empty, logging for diagnostics.
    let command = parts.next().ok_or_else(|| {
        error!("run_script is empty");
        CicdError::ScriptExecutionFailed("run_script configuration is empty".to_string())
    })?;
    let args: Vec<&str> = parts.collect();

    use std::fmt::Write as _;
    let mut cmd_str = String::new();
    write!(
        &mut cmd_str,
        "{}{}",
        command,
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

    // Log the full command being executed
    let mut full_command = String::from(command);
    for arg in &args {
        full_command.push(' ');
        full_command.push_str(arg);
    }
    info!("Running (cwd = '{}'): {}", repo_path, full_command);
    let script_output = Command::new(command)
        .current_dir(repo_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| {
            error!("run_script failed to start: {}", e);
            CicdError::ScriptExecutionFailed(format!(
                "Failed to start script '{}': {}. Ensure the command exists and is executable.",
                full_command, e
            ))
        })?;

    if script_output.status.success() {
        info!(
            "run_script output:\n{}",
            String::from_utf8_lossy(&script_output.stdout)
        );
        Ok(String::from_utf8_lossy(&script_output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&script_output.stderr);
        let stdout = String::from_utf8_lossy(&script_output.stdout);
        let exit_code = script_output.status.code().unwrap_or(-1);

        error!("run_script failed with exit code {}: {}", exit_code, stderr);

        Err(CicdError::ScriptExecutionFailed(format!(
            "Script '{}' failed with exit code {}.\nStderr: {}\nStdout: {}",
            full_command,
            exit_code,
            stderr.trim(),
            stdout.trim()
        )))
    }
}
