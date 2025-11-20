//! Jobs API endpoints

use axum::{
    Json,
    extract::{Path, Query, State as AxumState},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::SharedState;
use crate::job::{Job, JobStatus};

/// Query parameters for job listing
#[derive(Debug, Deserialize)]
pub struct JobsQuery {
    /// Filter by project name
    pub project: Option<String>,
    /// Filter by branch
    pub branch: Option<String>,
    /// Filter by status (queued, running, success, failed)
    pub status: Option<String>,
    /// Number of items per page (default: 50, max: 100)
    pub limit: Option<i64>,
    /// Offset for pagination (default: 0)
    pub offset: Option<i64>,
}

/// Response for paginated job listing
#[derive(Debug, Serialize)]
pub struct JobsResponse {
    pub jobs: Vec<Job>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// GET /api/jobs - Paginated job listing with filters
pub async fn get_jobs(
    AxumState(state): AxumState<SharedState>,
    Query(params): Query<JobsQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    // Get filtered jobs based on query params
    let result = if let Some(project) = &params.project {
        if let Some(branch) = &params.branch {
            state
                .job_store
                .get_jobs_by_branch(project, branch, limit)
                .await
        } else {
            state.job_store.get_jobs_by_project(project, limit).await
        }
    } else if let Some(status_str) = &params.status {
        let status = match status_str.to_lowercase().as_str() {
            "queued" => JobStatus::Queued,
            "running" => JobStatus::Running,
            "success" => JobStatus::Success,
            "failed" => JobStatus::Failed,
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Invalid status. Use: queued, running, success, failed"})),
                )
                    .into_response();
            }
        };
        state.job_store.get_jobs_by_status(status, limit).await
    } else {
        state.job_store.get_recent_jobs(limit).await
    };

    match result {
        Ok(jobs) => {
            let total = jobs.len() as i64;
            Json(JobsResponse {
                jobs,
                total,
                limit,
                offset,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// GET /api/jobs/{id} - Get a specific job by ID
pub async fn get_job(
    AxumState(state): AxumState<SharedState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.job_store.get_job(&id).await {
        Ok(Some(job)) => Json(job).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Job not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// GET /api/jobs/{id}/logs - Get structured logs for a job
pub async fn get_job_logs(
    AxumState(state): AxumState<SharedState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // First check if job exists
    match state.job_store.get_job(&id).await {
        Ok(Some(_)) => {
            // Job exists, get logs
            match state.job_store.get_job_logs(&id).await {
                Ok(logs) => Json(json!({
                    "job_id": id,
                    "logs": logs,
                    "count": logs.len()
                }))
                .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
                    .into_response(),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Job not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
