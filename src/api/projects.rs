//! Projects API endpoints

use axum::{
    Json,
    extract::State as AxumState,
};
use serde::Serialize;

use crate::job::JobStatus;
use crate::SharedState;

/// Summary of a project with recent job stats
#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub name: String,
    pub branches: Vec<String>,
    pub last_job_status: Option<String>,
    pub last_job_at: Option<String>,
    pub success_rate: f64,
    pub total_jobs: i64,
}

/// GET /api/projects - Get all projects with summaries
pub async fn get_projects(
    AxumState(state): AxumState<SharedState>,
) -> Json<serde_json::Value> {
    // Clone project configs to avoid holding lock across await
    let projects: Vec<_> = {
        let config = state.config.read().unwrap();
        config.project.iter().map(|p| (p.name.clone(), p.branches.clone())).collect()
    };

    let mut summaries = Vec::new();

    for (name, branches) in projects {
        // Get recent jobs for this project
        let jobs = state.job_store
            .get_jobs_by_project(&name, 10)
            .await
            .unwrap_or_default();

        let total_jobs = jobs.len() as i64;

        // Calculate success rate from recent jobs
        let success_count = jobs.iter()
            .filter(|j| j.status == JobStatus::Success)
            .count() as f64;
        let success_rate = if total_jobs > 0 {
            (success_count / total_jobs as f64) * 100.0
        } else {
            0.0
        };

        // Get last job info
        let (last_job_status, last_job_at) = jobs.first()
            .map(|j| {
                let status = match j.status {
                    JobStatus::Queued => "queued",
                    JobStatus::Running => "running",
                    JobStatus::Success => "success",
                    JobStatus::Failed => "failed",
                };
                (Some(status.to_string()), Some(j.started_at.to_rfc3339()))
            })
            .unwrap_or((None, None));

        summaries.push(ProjectSummary {
            name,
            branches,
            last_job_status,
            last_job_at,
            success_rate,
            total_jobs,
        });
    }

    Json(serde_json::json!({
        "projects": summaries,
        "count": summaries.len()
    }))
}
