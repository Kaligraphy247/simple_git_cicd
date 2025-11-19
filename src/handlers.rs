use axum::{
    Json,
    body::Bytes,
    extract::State as AxumState,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::json;
use simple_git_cicd::SharedState;
use simple_git_cicd::job::Job;
use simple_git_cicd::utils::{
    find_matching_project_owned, run_job_pipeline, verify_github_signature,
};
use std::collections::HashMap;
use tracing::{self, debug, error, info, warn};

pub async fn root() -> &'static str {
    "Hello, World!"
}

/// Returns the current server status with job information
/// Supports query parameters: ?project=name&status=failed&branch=main
pub async fn status(
    AxumState(state): AxumState<SharedState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let store = state.job_store.lock().await;
    let current = store.get_current_job();
    let queued = store.get_queued_count();

    // Filter jobs based on query parameters
    let jobs: Vec<_> = if let Some(project) = params.get("project") {
        if let Some(branch) = params.get("branch") {
            // Filter by project AND branch
            store.get_jobs_by_branch(project, branch)
        } else {
            // Filter by project only
            store.get_jobs_by_project(project)
        }
    } else if let Some(status_str) = params.get("status") {
        // Filter by status
        use simple_git_cicd::job::JobStatus;
        match status_str.to_lowercase().as_str() {
            "queued" => store.get_jobs_by_status(JobStatus::Queued),
            "running" => store.get_jobs_by_status(JobStatus::Running),
            "success" => store.get_jobs_by_status(JobStatus::Success),
            "failed" => store.get_jobs_by_status(JobStatus::Failed),
            _ => store.get_recent_jobs(10), // Invalid status, return recent
        }
    } else {
        // No filters, return recent 10
        store.get_recent_jobs(10)
    };

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
            "total_projects": state.config.project.len(),
        }
    }))
}

/// Returns a specific job by ID
pub async fn get_job(
    AxumState(state): AxumState<SharedState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let store = state.job_store.lock().await;
    match store.get_job(&id) {
        Some(job) => Json(job).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Job not found"})),
        )
            .into_response(),
    }
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
    let maybe_project = find_matching_project_owned(&state.config, repo_name, branch_name);

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

        // Create a new job
        let job = Job::new(repo_name.to_string(), branch_name.to_string());
        let job_id = job.id.clone();

        // Add job to store
        {
            let mut store = state.job_store.lock().await;
            store.add_job(job.clone());
        }

        info!(
            "Created job {} for project '{}' branch '{}'",
            job_id, repo_name, branch_name
        );

        // Clone/Extract necessary data to move into background task
        let repo_name = repo_name.to_owned();
        let branch_name = branch_name.to_owned();
        let repo_path = project.repo_path.clone();

        // Get shared state for background task
        let shared_state = state.clone();

        // Spawn a background async task to process job, which might be long running
        // I see you Nextjs..., but seriously tasks like rebuilding a docker image etc
        tokio::spawn(async move {
            // Acquire the job lock. Only one job will run at a time.
            // Good for servers with low resources, don't bait the OOM killer
            let _guard = shared_state.job_execution_lock.lock().await;

            // Mark job as running
            {
                let mut store = shared_state.job_store.lock().await;
                store.update_job(&job_id, |j| j.mark_running());
            }

            info!(
                "Job {} - Push event for project '{}' branch '{}'. Starting job pipeline.",
                job_id, repo_name, branch_name
            );

            let script = project.get_run_script_for_branch(&branch_name);

            // Run the job (git checkout, pull, then user script)
            match run_job_pipeline(&branch_name, &repo_path, script).await {
                Ok(output) => {
                    info!("Job {} completed successfully.", job_id);
                    let mut store = shared_state.job_store.lock().await;
                    store.update_job(&job_id, |j| j.mark_success(output));
                }
                Err(e) => {
                    error!("Job {} failed: {}", job_id, e);
                    let mut store = shared_state.job_store.lock().await;
                    store.update_job(&job_id, |j| j.mark_failed(e.to_string()));
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
