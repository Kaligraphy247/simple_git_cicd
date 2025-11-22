//! Webhook handler for GitHub push events

use axum::{
    body::Bytes,
    extract::Query,
    extract::State as AxumState,
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

use crate::SharedState;
use crate::api::stream::JobEvent;
use crate::db::store::JobLog;
use crate::job::{Job, JobStatus};
use crate::utils::{find_matching_project_owned, run_job_pipeline, verify_github_signature};
use crate::webhook::WebhookData;

/// Handles the GitHub webhook POST request.
pub async fn handle_webhook(
    AxumState(state): AxumState<SharedState>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    // Check for dry run mode
    let dry_run = params.get("dry_run").map(|v| v == "true").unwrap_or(false)
        || headers.get("X-Dry-Run").is_some();

    if cfg!(debug_assertions) && params.contains_key("dev") {
        debug!("Debug mode");
        debug!("Query Params: {:?}", params);
        return StatusCode::NO_CONTENT;
    }
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
    let maybe_project = {
        let config = state.config.read().unwrap();
        find_matching_project_owned(&config, repo_name, branch_name)
    };

    if let Some(project) = maybe_project {
        // check rate limits first
        let rate_limit_sec = project.get_rate_limit();
        let rate_limit_window = project.get_rate_limit_window();
        let mut rate_limiter = state.rate_limiter.lock().await;

        if rate_limiter.check_rate_limit(&project.name, rate_limit_sec, rate_limit_window) {
            warn!(
                "Too many requests for project {:?} - {:?} requests per {:?} seconds",
                &project.name, rate_limit_sec, rate_limit_window
            );
            return StatusCode::TOO_MANY_REQUESTS;
        }

        // Per-project webhook signature validation if required
        if project.needs_webhook_secret() {
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

        // Extract webhook data from payload
        let commit_sha = payload
            .get("after")
            .and_then(|v| v.as_str())
            .map(String::from);
        let commit_message = payload
            .get("head_commit")
            .and_then(|c| c.get("message"))
            .and_then(|v| v.as_str())
            .map(|s| {
                const MAX_COMMIT_MSG_LEN: usize = 500;
                if s.len() > MAX_COMMIT_MSG_LEN {
                    format!("{}... (truncated)", &s[..MAX_COMMIT_MSG_LEN])
                } else {
                    s.to_string()
                }
            });
        let commit_author_name = payload
            .get("head_commit")
            .and_then(|c| c.get("author"))
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .map(String::from);

        // Create a new job with webhook data
        let job = if dry_run {
            Job::from_webhook_dry_run(
                repo_name.to_string(),
                branch_name.to_string(),
                commit_sha.clone(),
                commit_message.clone(),
                commit_author_name.clone(),
            )
        } else {
            Job::from_webhook(
                repo_name.to_string(),
                branch_name.to_string(),
                commit_sha.clone(),
                commit_message.clone(),
                commit_author_name.clone(),
            )
        };
        let job_id = job.id.clone();

        // Add job to store
        if let Err(e) = state.job_store.create_job(&job).await {
            error!("Failed to create job in database: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }

        if dry_run {
            info!(
                "[DRY_RUN] Created job {} for project '{}' branch '{}'",
                job_id, repo_name, branch_name
            );
        } else {
            info!(
                "Created job {} for project '{}' branch '{}'",
                job_id, repo_name, branch_name
            );
        }

        // Broadcast job created event
        let _ = state.job_events.send(JobEvent {
            event_type: "created".to_string(),
            job_id: job_id.clone(),
            project_name: repo_name.to_string(),
            branch: branch_name.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        });

        // Build webhook data for pipeline
        let webhook_data = WebhookData {
            project_name: repo_name.to_string(),
            branch: branch_name.to_string(),
            repo_path: project.repo_path.clone(),
            commit_sha,
            commit_message,
            commit_author_name,
            commit_author_email: payload
                .get("head_commit")
                .and_then(|c| c.get("author"))
                .and_then(|a| a.get("email"))
                .and_then(|v| v.as_str())
                .map(String::from),
            pusher_name: payload
                .get("pusher")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from),
            repository_url: payload
                .get("repository")
                .and_then(|r| r.get("html_url"))
                .and_then(|v| v.as_str())
                .map(String::from),
        };

        // Get shared state for background task
        let shared_state = state.clone();

        // Spawn a background async task to process job
        tokio::spawn(async move {
            // Acquire the job lock. Only one job will run at a time.
            let _guard = shared_state.job_execution_lock.lock().await;

            // Mark job as running
            if let Err(e) = shared_state
                .job_store
                .update_job_status(&job_id, JobStatus::Running)
                .await
            {
                error!("Failed to update job status to running: {}", e);
                return;
            }

            // Handle dry run - skip actual execution
            if dry_run {
                info!(
                    "[DRY_RUN] Job {} - Would execute pipeline for project '{}' branch '{}'",
                    job_id, webhook_data.project_name, webhook_data.branch
                );

                let main_script = project.get_run_script_for_branch(&webhook_data.branch);
                let now = Utc::now();

                // Create simulated log entries for what would run
                let mut sequence = 0;

                // Git fetch
                let git_fetch_log = JobLog {
                    id: None,
                    job_id: job_id.clone(),
                    sequence,
                    log_type: "git_fetch".to_string(),
                    command: Some("git fetch origin".to_string()),
                    started_at: now,
                    completed_at: Some(now),
                    duration_ms: Some(0),
                    exit_code: Some(0),
                    output: Some("[DRY_RUN] Skipped".to_string()),
                    status: "skipped".to_string(),
                };
                let _ = shared_state.job_store.add_log(&git_fetch_log).await;
                sequence += 1;

                // Git reset
                let git_reset_log = JobLog {
                    id: None,
                    job_id: job_id.clone(),
                    sequence,
                    log_type: "git_reset".to_string(),
                    command: Some(format!("git reset --hard origin/{}", webhook_data.branch)),
                    started_at: now,
                    completed_at: Some(now),
                    duration_ms: Some(0),
                    exit_code: Some(0),
                    output: Some("[DRY_RUN] Skipped".to_string()),
                    status: "skipped".to_string(),
                };
                let _ = shared_state.job_store.add_log(&git_reset_log).await;
                sequence += 1;

                // Pre-script (if configured)
                if let Some(pre_script) = &project.pre_script {
                    let pre_log = JobLog {
                        id: None,
                        job_id: job_id.clone(),
                        sequence,
                        log_type: "pre_script".to_string(),
                        command: Some(pre_script.clone()),
                        started_at: now,
                        completed_at: Some(now),
                        duration_ms: Some(0),
                        exit_code: Some(0),
                        output: Some("[DRY_RUN] Skipped".to_string()),
                        status: "skipped".to_string(),
                    };
                    let _ = shared_state.job_store.add_log(&pre_log).await;
                    sequence += 1;
                }

                // Main script
                let main_log = JobLog {
                    id: None,
                    job_id: job_id.clone(),
                    sequence,
                    log_type: "main_script".to_string(),
                    command: Some(main_script.to_string()),
                    started_at: now,
                    completed_at: Some(now),
                    duration_ms: Some(0),
                    exit_code: Some(0),
                    output: Some("[DRY_RUN] Skipped".to_string()),
                    status: "skipped".to_string(),
                };
                let _ = shared_state.job_store.add_log(&main_log).await;
                sequence += 1;

                // Post-success script (if configured)
                if let Some(post_success) = &project.post_success_script {
                    let post_log = JobLog {
                        id: None,
                        job_id: job_id.clone(),
                        sequence,
                        log_type: "post_success_script".to_string(),
                        command: Some(post_success.clone()),
                        started_at: now,
                        completed_at: Some(now),
                        duration_ms: Some(0),
                        exit_code: Some(0),
                        output: Some("[DRY_RUN] Skipped".to_string()),
                        status: "skipped".to_string(),
                    };
                    let _ = shared_state.job_store.add_log(&post_log).await;
                    sequence += 1;
                }

                // Post-always script (if configured)
                if let Some(post_always) = &project.post_always_script {
                    let post_log = JobLog {
                        id: None,
                        job_id: job_id.clone(),
                        sequence,
                        log_type: "post_always_script".to_string(),
                        command: Some(post_always.clone()),
                        started_at: now,
                        completed_at: Some(now),
                        duration_ms: Some(0),
                        exit_code: Some(0),
                        output: Some("[DRY_RUN] Skipped".to_string()),
                        status: "skipped".to_string(),
                    };
                    let _ = shared_state.job_store.add_log(&post_log).await;
                    let _ = sequence; // silence unused warning
                }

                let dry_run_output = format!(
                    "[DRY_RUN] Pipeline simulation for project '{}' branch '{}'\n\
                     \n\
                     Webhook data:\n\
                     - Commit SHA: {}\n\
                     - Commit message: {}\n\
                     - Author: {}\n\
                     \n\
                     No actual commands were executed. See Timeline for details.",
                    webhook_data.project_name,
                    webhook_data.branch,
                    webhook_data.commit_sha.as_deref().unwrap_or("(none)"),
                    webhook_data.commit_message.as_deref().unwrap_or("(none)"),
                    webhook_data.commit_author_name.as_deref().unwrap_or("(none)"),
                );

                // Broadcast running event
                let _ = shared_state.job_events.send(JobEvent {
                    event_type: "running".to_string(),
                    job_id: job_id.clone(),
                    project_name: webhook_data.project_name.clone(),
                    branch: webhook_data.branch.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                });

                // Mark as success with dry run output
                if let Err(e) = shared_state
                    .job_store
                    .complete_job(&job_id, JobStatus::Success, Some(dry_run_output), None, Utc::now())
                    .await
                {
                    error!("[DRY_RUN] Failed to mark job as success: {}", e);
                }

                info!("[DRY_RUN] Job {} completed successfully.", job_id);
                let _ = shared_state.job_events.send(JobEvent {
                    event_type: "success".to_string(),
                    job_id: job_id.clone(),
                    project_name: webhook_data.project_name.clone(),
                    branch: webhook_data.branch.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                });

                return;
            }

            info!(
                "Job {} - Push event for project '{}' branch '{}'. Starting job pipeline.",
                job_id, webhook_data.project_name, webhook_data.branch
            );

            // Broadcast job running event
            let _ = shared_state.job_events.send(JobEvent {
                event_type: "running".to_string(),
                job_id: job_id.clone(),
                project_name: webhook_data.project_name.clone(),
                branch: webhook_data.branch.clone(),
                timestamp: Utc::now().to_rfc3339(),
            });

            // Run the complete pipeline with hooks
            match run_job_pipeline(
                &project,
                &webhook_data,
                &shared_state.job_store,
                &job_id,
                shared_state.log_chunks.clone(),
            )
            .await
            {
                Ok(output) => {
                    info!("Job {} completed successfully.", job_id);
                    if let Err(e) = shared_state
                        .job_store
                        .complete_job(&job_id, JobStatus::Success, Some(output), None, Utc::now())
                        .await
                    {
                        error!("Failed to mark job as success: {}", e);
                    }
                    let _ = shared_state.job_events.send(JobEvent {
                        event_type: "success".to_string(),
                        job_id: job_id.clone(),
                        project_name: webhook_data.project_name.clone(),
                        branch: webhook_data.branch.clone(),
                        timestamp: Utc::now().to_rfc3339(),
                    });
                }
                Err(e) => {
                    error!("Job {} failed: {}", job_id, e);
                    if let Err(db_err) = shared_state
                        .job_store
                        .complete_job(
                            &job_id,
                            JobStatus::Failed,
                            None,
                            Some(e.to_string()),
                            Utc::now(),
                        )
                        .await
                    {
                        error!("Failed to mark job as failed: {}", db_err);
                    }
                    let _ = shared_state.job_events.send(JobEvent {
                        event_type: "failed".to_string(),
                        job_id: job_id.clone(),
                        project_name: webhook_data.project_name.clone(),
                        branch: webhook_data.branch.clone(),
                        timestamp: Utc::now().to_rfc3339(),
                    });
                }
            }
        });

        StatusCode::OK
    } else {
        warn!(
            "No matching project for repo '{}' and branch '{}', skipping.",
            repo_name, branch_name
        );
        StatusCode::NO_CONTENT
    }
}
