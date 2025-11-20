//! Stats API endpoint

use axum::{
    Json,
    extract::{Query, State as AxumState},
    response::IntoResponse,
};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;

use crate::job::{Job, JobStatus};
use crate::SharedState;

/// Server statistics
#[derive(Debug, Serialize)]
pub struct ServerStats {
    pub name: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub started_at: String,
    pub total_projects: usize,
}

/// Job statistics
#[derive(Debug, Serialize)]
pub struct JobStats {
    pub total: i64,
    pub queued: i64,
    pub running: i64,
    pub success: i64,
    pub failed: i64,
    pub success_rate: f64,
}

/// Combined stats response
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub server: ServerStats,
    pub jobs: JobStats,
}

/// GET /api/stats - Get server and job statistics
pub async fn get_stats(
    AxumState(state): AxumState<SharedState>,
) -> Json<StatsResponse> {
    // Get project count without holding lock across await
    let total_projects = {
        let config = state.config.read().unwrap();
        config.project.len()
    };

    // Server stats
    let server = ServerStats {
        name: "simple_git_cicd".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
        started_at: state.started_at.to_rfc3339(),
        total_projects,
    };

    // Job stats - get counts for each status
    let queued = state.job_store.get_queued_count().await.unwrap_or(0);

    let running = state.job_store
        .get_jobs_by_status(JobStatus::Running, 1000)
        .await
        .map(|j| j.len() as i64)
        .unwrap_or(0);

    let success = state.job_store
        .get_jobs_by_status(JobStatus::Success, 1000)
        .await
        .map(|j| j.len() as i64)
        .unwrap_or(0);

    let failed = state.job_store
        .get_jobs_by_status(JobStatus::Failed, 1000)
        .await
        .map(|j| j.len() as i64)
        .unwrap_or(0);

    let total = queued + running + success + failed;
    let completed = success + failed;
    let success_rate = if completed > 0 {
        (success as f64 / completed as f64) * 100.0
    } else {
        0.0
    };

    let jobs = JobStats {
        total,
        queued,
        running,
        success,
        failed,
        success_rate,
    };

    Json(StatsResponse { server, jobs })
}

/// GET /api/status - Server status with job information
/// Supports query parameters: ?project=name&status=failed&branch=main
pub async fn status(
    AxumState(state): AxumState<SharedState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let current = state.job_store.get_current_job().await.ok().flatten();
    let queued = state.job_store.get_queued_count().await.unwrap_or(0);
    let completed = state.job_store.get_completed_count().await.unwrap_or(0);

    // Filter jobs based on query parameters
    let jobs: Vec<Job> = if let Some(project) = params.get("project") {
        if let Some(branch) = params.get("branch") {
            state
                .job_store
                .get_jobs_by_branch(project, branch, 50)
                .await
                .unwrap_or_default()
        } else {
            state
                .job_store
                .get_jobs_by_project(project, 50)
                .await
                .unwrap_or_default()
        }
    } else if let Some(status_str) = params.get("status") {
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
                .unwrap_or_default(),
        }
    } else {
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
            "completed_count": completed,
            "filtered": jobs,
            "filtered_count": jobs.len(),
        },
        "config": {
            "total_projects": config.project.len(),
        }
    }))
}
