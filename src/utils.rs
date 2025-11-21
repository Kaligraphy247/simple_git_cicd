use crate::api::stream::LogChunkEvent;
use crate::db::store::{JobLog, SqlJobStore};
use crate::error::{CicdError, Result};
use crate::webhook::WebhookData;
use crate::{CICDConfig, ProjectConfig};
use chrono::Utc;
use tokio::sync::broadcast;
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

/// Represents a running step with its database ID
pub struct RunningStep {
    pub id: i64,
    pub started_at: chrono::DateTime<Utc>,
}

/// Context for logging pipeline steps
pub struct PipelineLogger {
    job_store: SqlJobStore,
    job_id: String,
    sequence: i32,
    log_sender: broadcast::Sender<LogChunkEvent>,
}

impl PipelineLogger {
    pub fn new(job_store: SqlJobStore, job_id: String, log_sender: broadcast::Sender<LogChunkEvent>) -> Self {
        Self {
            job_store,
            job_id,
            sequence: 0,
            log_sender,
        }
    }

    /// Broadcast a log chunk via SSE
    fn broadcast_chunk(&self, step_type: &str, chunk: &str) {
        let _ = self.log_sender.send(LogChunkEvent {
            job_id: self.job_id.clone(),
            step_type: step_type.to_string(),
            chunk: chunk.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        });
    }

    /// Log a step that's about to start, returns the step handle for completion
    pub async fn start_step(&mut self, log_type: &str, command: Option<&str>) -> Option<RunningStep> {
        self.sequence += 1;
        let started_at = Utc::now();
        let log = JobLog {
            id: None,
            job_id: self.job_id.clone(),
            sequence: self.sequence,
            log_type: log_type.to_string(),
            command: command.map(String::from),
            started_at,
            completed_at: None,
            duration_ms: None,
            exit_code: None,
            output: None,
            status: "running".to_string(),
        };

        // Store the initial log entry
        match self.job_store.add_log(&log).await {
            Ok(id) => Some(RunningStep { id, started_at }),
            Err(e) => {
                error!("Failed to add log entry: {}", e);
                None
            }
        }
    }

    /// Complete a step with success
    pub async fn complete_step(&self, step: RunningStep, log_type: &str, output: String, exit_code: i32) {
        let completed_at = Utc::now();
        let duration_ms = (completed_at - step.started_at).num_milliseconds();

        // Broadcast the output via SSE
        if !output.is_empty() {
            self.broadcast_chunk(log_type, &output);
        }

        if let Err(e) = self
            .job_store
            .update_log(step.id, completed_at, duration_ms, exit_code, &output, "success")
            .await
        {
            error!("Failed to update log entry: {}", e);
        }
    }

    /// Complete a step with failure
    pub async fn fail_step(&self, step: RunningStep, log_type: &str, output: String, exit_code: i32) {
        let completed_at = Utc::now();
        let duration_ms = (completed_at - step.started_at).num_milliseconds();

        // Broadcast the output via SSE
        if !output.is_empty() {
            self.broadcast_chunk(log_type, &output);
        }

        if let Err(e) = self
            .job_store
            .update_log(step.id, completed_at, duration_ms, exit_code, &output, "failed")
            .await
        {
            error!("Failed to update log entry: {}", e);
        }
    }
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

/// Helper to run the complete CI/CD pipeline with hooks
/// Returns combined stdout/stderr output or error.
pub async fn run_job_pipeline(
    project: &ProjectConfig,
    webhook_data: &WebhookData,
    job_store: &SqlJobStore,
    job_id: &str,
    log_sender: broadcast::Sender<LogChunkEvent>,
) -> Result<String> {
    let branch = &webhook_data.branch;
    let repo_path = &webhook_data.repo_path;
    let reset_to_remote = project.should_reset_to_remote();
    use tokio::process::Command;
    use tracing::{error, info};

    let mut logger = PipelineLogger::new(job_store.clone(), job_id.to_string(), log_sender);
    let mut all_output = String::new();

    // 1. git fetch to update remote refs
    let step = logger.start_step("git_fetch", Some("git fetch")).await;
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
    let fetch_output = format!(
        "{}{}",
        String::from_utf8_lossy(&fetch.stdout),
        String::from_utf8_lossy(&fetch.stderr)
    );
    if !fetch.status.success() {
        error!("git fetch failed: {}", fetch_output);
        if let Some(s) = step {
            logger.fail_step(s, "git_fetch", fetch_output.clone(), fetch.status.code().unwrap_or(-1)).await;
        }
        return Err(CicdError::GitOperationFailed {
            operation: "git fetch".to_string(),
            message: format!(
                "{}. Check network connectivity and repository access.",
                fetch_output.trim()
            ),
        });
    }
    if let Some(s) = step {
        logger.complete_step(s, "git_fetch", fetch_output.clone(), 0).await;
    }
    all_output.push_str(&fetch_output);
    info!("git fetch output:\n{}", fetch_output);

    // 2. Reset to remote or switch+pull
    if reset_to_remote {
        // CI/CD mode: Hard reset to match remote exactly (handles modified files)
        let reset_cmd = format!("git reset --hard origin/{}", branch);
        let step = logger.start_step("git_reset", Some(&reset_cmd)).await;
        info!("Resetting to remote state (reset_to_remote=true)");
        info!("Running (cwd = '{}'): {}", repo_path, reset_cmd);

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["reset", "--hard", &format!("origin/{}", branch)])
            .output()
            .await
            .map_err(|e| {
                error!("git reset --hard failed to start: {}", e);
                CicdError::GitOperationFailed {
                    operation: "git reset --hard".to_string(),
                    message: format!("Failed to start git process: {}", e),
                }
            })?;

        let reset_output = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        if !output.status.success() {
            error!("git reset --hard failed: {}", reset_output);
            if let Some(s) = step {
                logger.fail_step(s, "git_reset", reset_output.clone(), output.status.code().unwrap_or(-1)).await;
            }
            return Err(CicdError::GitOperationFailed {
                operation: format!("git reset --hard origin/{}", branch),
                message: format!("{}. Ensure the target 'origin/{}' exists.", reset_output.trim(), branch),
            });
        }

        if let Some(s) = step {
            logger.complete_step(s, "git_reset", reset_output.clone(), 0).await;
        }
        all_output.push_str(&reset_output);
        info!("git reset --hard output:\n{}", reset_output);
    } else {
        // Debug mode: Normal switch + pull
        info!("Using switch + pull mode (reset_to_remote=false)");

        // 2a. git switch to branch
        let switch_cmd = format!("git switch {}", branch);
        let step = logger.start_step("git_switch", Some(&switch_cmd)).await;
        info!("Running (cwd = '{}'): {}", repo_path, switch_cmd);
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
        let switch_output = format!(
            "{}{}",
            String::from_utf8_lossy(&checkout.stdout),
            String::from_utf8_lossy(&checkout.stderr)
        );
        if !checkout.status.success() {
            error!("git switch failed: {}", switch_output);
            if let Some(s) = step {
                logger.fail_step(s, "git_switch", switch_output.clone(), checkout.status.code().unwrap_or(-1)).await;
            }
            return Err(CicdError::GitOperationFailed {
                operation: format!("git switch {}", branch),
                message: format!(
                    "{}. Ensure branch '{}' exists remotely.",
                    switch_output.trim(),
                    branch
                ),
            });
        }
        if let Some(s) = step {
            logger.complete_step(s, "git_switch", switch_output.clone(), 0).await;
        }
        all_output.push_str(&switch_output);
        info!("git switch output:\n{}", switch_output);

        // 2b. git pull
        let step = logger.start_step("git_pull", Some("git pull")).await;
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
        let pull_output = format!(
            "{}{}",
            String::from_utf8_lossy(&pull.stdout),
            String::from_utf8_lossy(&pull.stderr)
        );
        if !pull.status.success() {
            error!("git pull failed: {}", pull_output);
            if let Some(s) = step {
                logger.fail_step(s, "git_pull", pull_output.clone(), pull.status.code().unwrap_or(-1)).await;
            }
            return Err(CicdError::GitOperationFailed {
                operation: "git pull".to_string(),
                message: format!(
                    "{}. Ensure there are no local changes or merge conflicts.",
                    pull_output.trim()
                ),
            });
        }
        if let Some(s) = step {
            logger.complete_step(s, "git_pull", pull_output.clone(), 0).await;
        }
        all_output.push_str(&pull_output);
        info!("git pull output:\n{}", pull_output);
    }

    // 3. Run pre-script if configured
    if let Some(pre_script) = &project.pre_script {
        let step = logger.start_step("pre_script", Some(pre_script)).await;
        info!("Running pre-script: {}", pre_script);
        match run_script_with_env(pre_script, repo_path, webhook_data, None).await {
            Ok(result) => {
                if let Some(s) = step {
                    logger.complete_step(s, "pre_script", result.output.clone(), result.exit_code).await;
                }
                all_output.push_str(&result.output);
            }
            Err(e) => {
                if let Some(s) = step {
                    logger.fail_step(s, "pre_script", e.to_string(), 1).await;
                }
                return Err(e);
            }
        }
    }

    // 4. Run main script
    let main_script = project.get_run_script_for_branch(branch);
    let step = logger.start_step("main_script", Some(main_script)).await;
    info!("Running main script: {}", main_script);
    let main_result = run_script_with_env(main_script, repo_path, webhook_data, None).await;
    let main_exit_code = main_result.as_ref().map(|r| r.exit_code).unwrap_or(1);

    match &main_result {
        Ok(result) => {
            if let Some(s) = step {
                logger.complete_step(s, "main_script", result.output.clone(), result.exit_code).await;
            }
            all_output.push_str(&result.output);
        }
        Err(e) => {
            if let Some(s) = step {
                logger.fail_step(s, "main_script", e.to_string(), main_exit_code).await;
            }
        }
    }

    // 5. Run post scripts based on main script result
    let post_env = Some(("CICD_MAIN_SCRIPT_EXIT_CODE", main_exit_code.to_string()));

    match &main_result {
        Ok(_) => {
            // Success path
            if let Some(script) = &project.post_success_script {
                let step = logger.start_step("post_success", Some(script)).await;
                info!("Running post-success script: {}", script);
                match run_script_with_env(script, repo_path, webhook_data, post_env.clone()).await {
                    Ok(result) => {
                        if let Some(s) = step {
                            logger.complete_step(s, "post_success", result.output.clone(), result.exit_code).await;
                        }
                        all_output.push_str(&result.output);
                    }
                    Err(e) => {
                        if let Some(s) = step {
                            logger.fail_step(s, "post_success", e.to_string(), 1).await;
                        }
                    }
                }
            } else if let Some(script) = &project.post_script {
                let step = logger.start_step("post_script", Some(script)).await;
                info!("Running post script (after success): {}", script);
                match run_script_with_env(script, repo_path, webhook_data, post_env.clone()).await {
                    Ok(result) => {
                        if let Some(s) = step {
                            logger.complete_step(s, "post_script", result.output.clone(), result.exit_code).await;
                        }
                        all_output.push_str(&result.output);
                    }
                    Err(e) => {
                        if let Some(s) = step {
                            logger.fail_step(s, "post_script", e.to_string(), 1).await;
                        }
                    }
                }
            }
        }
        Err(_) => {
            // Failure path
            if let Some(script) = &project.post_failure_script {
                let step = logger.start_step("post_failure", Some(script)).await;
                info!("Running post-failure script: {}", script);
                match run_script_with_env(script, repo_path, webhook_data, post_env.clone()).await {
                    Ok(result) => {
                        if let Some(s) = step {
                            logger.complete_step(s, "post_failure", result.output.clone(), result.exit_code).await;
                        }
                        all_output.push_str(&result.output);
                    }
                    Err(e) => {
                        if let Some(s) = step {
                            logger.fail_step(s, "post_failure", e.to_string(), 1).await;
                        }
                    }
                }
            } else if let Some(script) = &project.post_script {
                let step = logger.start_step("post_script", Some(script)).await;
                info!("Running post script (after failure): {}", script);
                match run_script_with_env(script, repo_path, webhook_data, post_env.clone()).await {
                    Ok(result) => {
                        if let Some(s) = step {
                            logger.complete_step(s, "post_script", result.output.clone(), result.exit_code).await;
                        }
                        all_output.push_str(&result.output);
                    }
                    Err(e) => {
                        if let Some(s) = step {
                            logger.fail_step(s, "post_script", e.to_string(), 1).await;
                        }
                    }
                }
            }
        }
    }

    // 6. Always run post_always_script
    if let Some(script) = &project.post_always_script {
        let step = logger.start_step("post_always", Some(script)).await;
        info!("Running post-always script: {}", script);
        match run_script_with_env(script, repo_path, webhook_data, post_env).await {
            Ok(result) => {
                if let Some(s) = step {
                    logger.complete_step(s, "post_always", result.output.clone(), result.exit_code).await;
                }
                all_output.push_str(&result.output);
            }
            Err(e) => {
                if let Some(s) = step {
                    logger.fail_step(s, "post_always", e.to_string(), 1).await;
                }
            }
        }
    }

    // 7. Return main script result (or all output on success)
    main_result.map(|_| all_output)
}
