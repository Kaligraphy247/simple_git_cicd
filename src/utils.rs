use crate::error::{CicdError, Result};
use crate::webhook::WebhookData;
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

/// Result of script execution with output and exit code
#[derive(Debug)]
pub struct ScriptResult {
    pub output: String,
    pub exit_code: i32,
}

/// Run a script with environment variables from webhook data
/// Optionally pass extra environment variables (e.g., CICD_MAIN_SCRIPT_EXIT_CODE)
async fn run_script_with_env(
    script: &str,
    repo_path: &str,
    webhook_data: &WebhookData,
    extra_env: Option<(&str, String)>,
) -> Result<ScriptResult> {
    use tokio::process::Command;

    // Parse script into command and args
    let mut parts = script.split_whitespace();
    let command = parts.next().ok_or_else(|| {
        error!("Script is empty");
        CicdError::ScriptExecutionFailed("Script configuration is empty".to_string())
    })?;
    let args: Vec<&str> = parts.collect();

    // Build full command string for logging
    let mut full_command = String::from(command);
    for arg in &args {
        full_command.push(' ');
        full_command.push_str(arg);
    }

    info!("Running (cwd = '{}'): {}", repo_path, full_command);

    // Build command with environment variables
    let mut cmd = Command::new(command);
    cmd.current_dir(repo_path)
        .args(&args)
        .env("CICD_PROJECT_NAME", &webhook_data.project_name)
        .env("CICD_BRANCH", &webhook_data.branch)
        .env("CICD_REPO_PATH", &webhook_data.repo_path);

    // Add optional webhook data as env vars
    if let Some(sha) = &webhook_data.commit_sha {
        cmd.env("CICD_COMMIT_SHA", sha);
    }
    if let Some(msg) = &webhook_data.commit_message {
        cmd.env("CICD_COMMIT_MESSAGE", msg);
    }
    if let Some(name) = &webhook_data.commit_author_name {
        cmd.env("CICD_COMMIT_AUTHOR_NAME", name);
    }
    if let Some(email) = &webhook_data.commit_author_email {
        cmd.env("CICD_COMMIT_AUTHOR_EMAIL", email);
    }
    if let Some(pusher) = &webhook_data.pusher_name {
        cmd.env("CICD_PUSHER_NAME", pusher);
    }
    if let Some(url) = &webhook_data.repository_url {
        cmd.env("CICD_REPOSITORY_URL", url);
    }

    // Add extra environment variable if provided
    if let Some((key, value)) = extra_env {
        cmd.env(key, value);
    }

    // Execute command
    let output = cmd.output().await.map_err(|e| {
        error!("Script failed to start: {}", e);
        CicdError::ScriptExecutionFailed(format!(
            "Failed to start script '{}': {}. Ensure the command exists and is executable.",
            full_command, e
        ))
    })?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Combine stdout and stderr for output
    let combined_output = if !stderr.is_empty() {
        format!("{}\n{}", stdout, stderr)
    } else {
        stdout
    };

    if output.status.success() {
        info!("Script completed successfully");
        Ok(ScriptResult {
            output: combined_output,
            exit_code,
        })
    } else {
        error!("Script failed with exit code {}", exit_code);
        Err(CicdError::ScriptExecutionFailed(format!(
            "Script '{}' failed with exit code {}.\nOutput: {}",
            full_command,
            exit_code,
            combined_output.trim()
        )))
    }
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

/// Helper to run the complete CI/CD pipeline with hooks
/// Returns combined stdout/stderr output or error.
pub async fn run_job_pipeline(
    project: &ProjectConfig,
    webhook_data: &WebhookData,
) -> Result<String> {
    let branch = &webhook_data.branch;
    let repo_path = &webhook_data.repo_path;
    let reset_to_remote = project.should_reset_to_remote();
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

    // 3. Run pre-script if configured
    if let Some(pre_script) = &project.pre_script {
        info!("Running pre-script: {}", pre_script);
        run_script_with_env(pre_script, repo_path, webhook_data, None).await?;
    }

    // 4. Run main script
    let main_script = project.get_run_script_for_branch(branch);
    info!("Running main script: {}", main_script);
    let main_result = run_script_with_env(main_script, repo_path, webhook_data, None).await;
    let main_exit_code = main_result.as_ref().map(|r| r.exit_code).unwrap_or(1);

    // 5. Run post scripts based on main script result
    let post_env = Some(("CICD_MAIN_SCRIPT_EXIT_CODE", main_exit_code.to_string()));

    match &main_result {
        Ok(_) => {
            // Success path
            if let Some(script) = &project.post_success_script {
                info!("Running post-success script: {}", script);
                let _ = run_script_with_env(script, repo_path, webhook_data, post_env.clone()).await;
            } else if let Some(script) = &project.post_script {
                info!("Running post script (after success): {}", script);
                let _ = run_script_with_env(script, repo_path, webhook_data, post_env.clone()).await;
            }
        }
        Err(_) => {
            // Failure path
            if let Some(script) = &project.post_failure_script {
                info!("Running post-failure script: {}", script);
                let _ = run_script_with_env(script, repo_path, webhook_data, post_env.clone()).await;
            } else if let Some(script) = &project.post_script {
                info!("Running post script (after failure): {}", script);
                let _ = run_script_with_env(script, repo_path, webhook_data, post_env.clone()).await;
            }
        }
    }

    // 6. Always run post_always_script
    if let Some(script) = &project.post_always_script {
        info!("Running post-always script: {}", script);
        let _ = run_script_with_env(script, repo_path, webhook_data, post_env).await;
    }

    // 7. Return main script result
    main_result.map(|r| r.output)
}
