use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

/// Maximum size for job output before truncation (1MB)
pub const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

/// Represents the status of a CI/CD job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Success,
    Failed,
}

/// Represents a CI/CD job with its metadata and execution details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub project_name: String,
    pub branch: String,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub commit_author: Option<String>,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub output: Option<String>,
    pub output_truncated: bool,
    pub error: Option<String>,
}

impl Job {
    /// Create a new job in Queued status
    pub fn new(project_name: String, branch: String) -> Self {
        Self {
            id: Uuid::now_v7().to_string(),
            project_name,
            branch,
            commit_sha: None,
            commit_message: None,
            commit_author: None,
            status: JobStatus::Queued,
            started_at: Utc::now(),
            completed_at: None,
            output: None,
            output_truncated: false,
            error: None,
        }
    }

    /// Mark job as running
    pub fn mark_running(&mut self) {
        self.status = JobStatus::Running;
    }

    /// Mark job as successful with output (truncates if too large)
    pub fn mark_success(&mut self, mut output: String) {
        self.status = JobStatus::Success;
        self.completed_at = Some(Utc::now());

        // Truncate output if it's too large
        if output.len() > MAX_OUTPUT_SIZE {
            output.truncate(MAX_OUTPUT_SIZE);
            output.push_str("\n... (output truncated)");
            self.output_truncated = true;
        }

        self.output = Some(output);
    }

    /// Mark job as failed with error
    pub fn mark_failed(&mut self, error: String) {
        self.status = JobStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error = Some(error);
    }
}

/// In-memory store for jobs with a configurable size limit
pub struct JobStore {
    jobs: VecDeque<Job>,
    max_jobs: usize,
}

impl JobStore {
    /// Create a new JobStore with a maximum number of jobs to retain
    pub fn new(max_jobs: usize) -> Self {
        Self {
            jobs: VecDeque::with_capacity(max_jobs),
            max_jobs,
        }
    }

    /// Add a new job to the store, removing oldest if at capacity
    pub fn add_job(&mut self, job: Job) {
        if self.jobs.len() >= self.max_jobs {
            self.jobs.pop_front();
        }
        self.jobs.push_back(job);
    }

    /// Update a job's status by ID
    pub fn update_job<F>(&mut self, id: &str, update_fn: F)
    where
        F: FnOnce(&mut Job),
    {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
            update_fn(job);
        }
    }

    /// Get a job by ID
    pub fn get_job(&self, id: &str) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }

    /// Get the currently running job (if any)
    pub fn get_current_job(&self) -> Option<&Job> {
        self.jobs
            .iter()
            .rev()
            .find(|j| j.status == JobStatus::Running)
    }

    /// Get count of queued jobs
    pub fn get_queued_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| j.status == JobStatus::Queued)
            .count()
    }

    /// Get the most recent N jobs
    pub fn get_recent_jobs(&self, limit: usize) -> Vec<&Job> {
        self.jobs.iter().rev().take(limit).collect()
    }

    /// Get count of completed jobs (success + failed)
    pub fn get_completed_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| {
                j.status == JobStatus::Success || j.status == JobStatus::Failed
            })
            .count()
    }

    /// Get jobs by project name
    pub fn get_jobs_by_project(&self, project_name: &str) -> Vec<&Job> {
        self.jobs
            .iter()
            .filter(|j| j.project_name == project_name)
            .collect()
    }

    /// Get jobs by status
    pub fn get_jobs_by_status(&self, status: JobStatus) -> Vec<&Job> {
        self.jobs.iter().filter(|j| j.status == status).collect()
    }

    /// Get jobs by project and branch
    pub fn get_jobs_by_branch(&self, project: &str, branch: &str) -> Vec<&Job> {
        self.jobs
            .iter()
            .filter(|j| j.project_name == project && j.branch == branch)
            .collect()
    }

    /// Get all failed jobs
    pub fn get_failed_jobs(&self) -> Vec<&Job> {
        self.jobs
            .iter()
            .filter(|j| j.status == JobStatus::Failed)
            .collect()
    }
}
