//! Core HTTP handlers for webhook processing and server status

use axum::{
    Json,
    body::Bytes,
    extract::Query,
    extract::State as AxumState,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use tracing::{self, debug, error, info, warn};

use crate::api::stream::JobEvent;
use crate::job::{Job, JobStatus};
use crate::utils::{find_matching_project_owned, run_job_pipeline, verify_github_signature};
use crate::webhook::WebhookData;
use crate::{SharedState, reload_config};

/// Root health check endpoint
/// Supports ?format=json for detailed JSON response
pub async fn root(
    AxumState(state): AxumState<SharedState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let format = params.get("format").map(|s| s.as_str());

    if format == Some("json") {
        let current_job = state.job_store.get_current_job().await.ok().flatten();
        let completed_count = state.job_store.get_completed_count().await.unwrap_or(0);
        let config = state.config.read().unwrap();

        Json(json!({
            "name": "simple_git_cicd",
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_seconds": state.start_time.elapsed().as_secs(),
            "current_job": current_job.map(|j| j.id),
            "total_projects": config.project.len(),
            "jobs_completed": completed_count,
            "status": "healthy"
        }))
        .into_response()
    } else {
        "simple_git_cicd - healthy".into_response()
    }
}

/// Returns the current server status with job information
/// Supports query parameters: ?project=name&status=failed&branch=main
pub async fn status(
    AxumState(state): AxumState<SharedState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let current = state.job_store.get_current_job().await.ok().flatten();
    let queued = state.job_store.get_queued_count().await.unwrap_or(0);

    // Filter jobs based on query parameters
    let jobs: Vec<Job> = if let Some(project) = params.get("project") {
        if let Some(branch) = params.get("branch") {
            // Filter by project AND branch
            state
                .job_store
                .get_jobs_by_branch(project, branch, 50)
                .await
                .unwrap_or_default()
        } else {
            // Filter by project only
            state
                .job_store
                .get_jobs_by_project(project, 50)
                .await
                .unwrap_or_default()
        }
    } else if let Some(status_str) = params.get("status") {
        // Filter by status
        match status_str.to_lowercase().as_str() {
            "queued" => state
                .job_store
                .get_jobs_by_status(JobStatus::Queued, 50)
                .await
                .unwrap_or_default(),
            "running" => state
                .job_store
                .get_jobs_by_status(JobStatus::Running, 50)
                .await
                .unwrap_or_default(),
            "success" => state
                .job_store
                .get_jobs_by_status(JobStatus::Success, 50)
                .await
                .unwrap_or_default(),
            "failed" => state
                .job_store
                .get_jobs_by_status(JobStatus::Failed, 50)
                .await
                .unwrap_or_default(),
            _ => state
                .job_store
                .get_recent_jobs(10)
                .await
                .unwrap_or_default(), // Invalid status, return recent
        }
    } else {
        // No filters, return recent 10
        state
            .job_store
            .get_recent_jobs(10)
            .await
            .unwrap_or_default()
    };

    let config = state.config.read().unwrap();

    Json(json!({
        "server": {
            "name": "simple_git_cicd",
            "version": env!("CARGO_PKG_VERSION"),
            "started_at": state.started_at,
            "uptime_seconds": state.start_time.elapsed().as_secs(),
        },
        "jobs": {
            "current": current,
            "queued_count": queued,
            "filtered": jobs,
            "filtered_count": jobs.len(),
        },
        "config": {
            "total_projects": config.project.len(),
        }
    }))
}

/// Handles the GitHub webhook POST request.
pub async fn handle_webhook(
    AxumState(state): AxumState<SharedState>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
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
    let branch_ref = branch_ref.unwrap(); // full reference to the branch_ref
    let branch_name = branch_ref.strip_prefix("refs/heads/").unwrap_or(branch_ref);
    let repo_name = repo_name.unwrap();

    // Find matching project config based on repo name and branch
    // Clone the project config and drop the lock immediately
    let maybe_project = {
        let config = state.config.read().unwrap();
        find_matching_project_owned(&config, repo_name, branch_name)
    };

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
                // Truncate commit messages to 500 chars (they can be very long for squashed commits)
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
        let job = Job::from_webhook(
            repo_name.to_string(),
            branch_name.to_string(),
            commit_sha.clone(),
            commit_message.clone(),
            commit_author_name.clone(),
        );
        let job_id = job.id.clone();

        // Add job to store
        if let Err(e) = state.job_store.create_job(&job).await {
            error!("Failed to create job in database: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }

        info!(
            "Created job {} for project '{}' branch '{}'",
            job_id, repo_name, branch_name
        );

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

        // Spawn a background async task to process job, which might be long running
        // I see you Nextjs..., but seriously tasks like rebuilding a docker image etc
        tokio::spawn(async move {
            // Acquire the job lock. Only one job will run at a time.
            // Good for servers with low resources, don't bait the OOM killer
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
            match run_job_pipeline(&project, &webhook_data).await {
                Ok(output) => {
                    info!("Job {} completed successfully.", job_id);
                    if let Err(e) = shared_state
                        .job_store
                        .complete_job(&job_id, JobStatus::Success, Some(output), None, Utc::now())
                        .await
                    {
                        error!("Failed to mark job as success: {}", e);
                    }
                    // Broadcast success event
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
                    // Broadcast failed event
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

        // Return immediately so Github webhook request responds within 10 seconds
        StatusCode::OK
    } else {
        warn!(
            "No matching project for repo '{}' and branch '{}', skipping.",
            repo_name, branch_name
        );
        StatusCode::NO_CONTENT
    }
}

/// Reload configuration from disk
/// Waits for current job to finish before applying the new config
pub async fn reload_config_endpoint(AxumState(state): AxumState<SharedState>) -> impl IntoResponse {
    // Wait for current job to finish before reloading
    let _guard = state.job_execution_lock.lock().await;

    match reload_config(&state.config_path).await {
        Ok(new_config) => {
            // Acquire write lock on shared config
            let mut config = state.config.write().unwrap();
            // Dereference guard (*config) to overwrite the inner data
            *config = new_config;
            info!(
                "Configuration reloaded successfully from {:?}",
                state.config_path
            );
            Json(json!({
                "status": "success",
                "message": "Configuration reloaded successfully"
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to reload config: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": e.to_string()
                })),
            )
                .into_response()
        }
    }
}
