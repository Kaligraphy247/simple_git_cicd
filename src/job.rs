use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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

    /// Create a new job from webhook data
    pub fn from_webhook(
        project_name: String,
        branch: String,
        commit_sha: Option<String>,
        commit_message: Option<String>,
        commit_author: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::now_v7().to_string(),
            project_name,
            branch,
            commit_sha,
            commit_message,
            commit_author,
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
