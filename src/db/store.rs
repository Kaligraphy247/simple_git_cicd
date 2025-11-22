use crate::error::CicdError;
use crate::job::{Job, JobStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Represents a structured log entry for a specific step in a job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLog {
    pub id: Option<i64>, // Auto-increment
    pub job_id: String,
    pub sequence: i32,
    pub log_type: String, // git_fetch, main_script, etc.
    pub command: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
    pub status: String, // running, success, failed
}

// Helper struct to map DB row to JobLog struct
#[derive(FromRow)]
struct JobLogRow {
    id: Option<i64>,
    job_id: String,
    sequence: i32,
    log_type: String,
    command: Option<String>,
    started_at: String,
    completed_at: Option<String>,
    duration_ms: Option<i64>,
    exit_code: Option<i32>,
    output: Option<String>,
    status: String,
}

impl From<JobLogRow> for JobLog {
    fn from(row: JobLogRow) -> Self {
        let started_at = DateTime::parse_from_rfc3339(&row.started_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let completed_at = row.completed_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        JobLog {
            id: row.id,
            job_id: row.job_id,
            sequence: row.sequence,
            log_type: row.log_type,
            command: row.command,
            started_at,
            completed_at,
            duration_ms: row.duration_ms,
            exit_code: row.exit_code,
            output: row.output,
            status: row.status,
        }
    }
}

/// Persistent storage for jobs using SQLite
#[derive(Clone)]
pub struct SqlJobStore {
    pool: SqlitePool,
}

impl SqlJobStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new job record
    pub async fn create_job(&self, job: &Job) -> Result<(), CicdError> {
        let status_str = serde_json::to_string(&job.status)
            .unwrap_or_else(|_| "queued".to_string())
            .replace('"', "");

        sqlx::query(
            r#"
            INSERT INTO jobs (
                id, project_name, branch, status,
                commit_sha, commit_message, commit_author_name,
                started_at, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&job.id)
        .bind(&job.project_name)
        .bind(&job.branch)
        .bind(status_str)
        .bind(&job.commit_sha)
        .bind(&job.commit_message)
        .bind(&job.commit_author)
        .bind(job.started_at.to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to create job: {}", e)))?;

        Ok(())
    }

    /// Update job status
    pub async fn update_job_status(&self, id: &str, status: JobStatus) -> Result<(), CicdError> {
        let status_str = serde_json::to_string(&status)
            .unwrap_or_else(|_| "failed".to_string())
            .replace('"', "");

        sqlx::query("UPDATE jobs SET status = ? WHERE id = ?")
            .bind(status_str)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CicdError::DatabaseError(format!("Failed to update job status: {}", e)))?;

        Ok(())
    }

    /// Complete a job (success or failure)
    pub async fn complete_job(
        &self,
        id: &str,
        status: JobStatus,
        output: Option<String>,
        error: Option<String>,
        completed_at: DateTime<Utc>,
    ) -> Result<(), CicdError> {
        let status_str = serde_json::to_string(&status)
            .unwrap_or_else(|_| "failed".to_string())
            .replace('"', "");

        // Fetch started_at to calculate duration in Rust
        let started_at: (String,) = sqlx::query_as("SELECT started_at FROM jobs WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                CicdError::DatabaseError(format!("Failed to fetch job started_at: {}", e))
            })?;

        // Parse started_at and calculate duration
        let duration_ms = DateTime::parse_from_rfc3339(&started_at.0)
            .map(|start| (completed_at - start.with_timezone(&Utc)).num_milliseconds())
            .unwrap_or(0);

        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                output = ?,
                error = ?,
                completed_at = ?,
                duration_ms = ?
            WHERE id = ?
            "#,
        )
        .bind(status_str)
        .bind(output)
        .bind(error)
        .bind(completed_at.to_rfc3339())
        .bind(duration_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to complete job: {}", e)))?;

        Ok(())
    }

    /// Get a job by ID
    pub async fn get_job(&self, id: &str) -> Result<Option<Job>, CicdError> {
        let row = sqlx::query_as::<_, JobRow>(
            r#"
            SELECT
                id, project_name, branch, status,
                commit_sha, commit_message, commit_author_name,
                started_at, completed_at, output, output_truncated, error
            FROM jobs
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to fetch job: {}", e)))?;

        Ok(row.map(|r| r.into()))
    }

    /// Get recent jobs
    pub async fn get_recent_jobs(&self, limit: i64) -> Result<Vec<Job>, CicdError> {
        let rows = sqlx::query_as::<_, JobRow>(
            r#"
            SELECT
                id, project_name, branch, status,
                commit_sha, commit_message, commit_author_name,
                started_at, completed_at, output, output_truncated, error
            FROM jobs
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to fetch recent jobs: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get jobs by project
    pub async fn get_jobs_by_project(
        &self,
        project: &str,
        limit: i64,
    ) -> Result<Vec<Job>, CicdError> {
        let rows = sqlx::query_as::<_, JobRow>(
            r#"
            SELECT
                id, project_name, branch, status,
                commit_sha, commit_message, commit_author_name,
                started_at, completed_at, output, output_truncated, error
            FROM jobs
            WHERE project_name = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(project)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to fetch project jobs: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Add a log entry for a job step, returns the inserted ID
    pub async fn add_log(&self, log: &JobLog) -> Result<i64, CicdError> {
        let result = sqlx::query(
            r#"
            INSERT INTO job_logs (
                job_id, sequence, log_type, command,
                started_at, completed_at, duration_ms,
                exit_code, output, status
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&log.job_id)
        .bind(log.sequence)
        .bind(&log.log_type)
        .bind(&log.command)
        .bind(log.started_at.to_rfc3339())
        .bind(log.completed_at.map(|dt| dt.to_rfc3339()))
        .bind(log.duration_ms)
        .bind(log.exit_code)
        .bind(&log.output)
        .bind(&log.status)
        .execute(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to add job log: {}", e)))?;

        Ok(result.last_insert_rowid())
    }

    /// Update an existing log entry (for completing a step)
    pub async fn update_log(
        &self,
        id: i64,
        completed_at: DateTime<Utc>,
        duration_ms: i64,
        exit_code: i32,
        output: &str,
        status: &str,
    ) -> Result<(), CicdError> {
        sqlx::query(
            r#"
            UPDATE job_logs
            SET completed_at = ?, duration_ms = ?, exit_code = ?, output = ?, status = ?
            WHERE id = ?
            "#,
        )
        .bind(completed_at.to_rfc3339())
        .bind(duration_ms)
        .bind(exit_code)
        .bind(output)
        .bind(status)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to update job log: {}", e)))?;

        Ok(())
    }

    /// Get logs for a job
    pub async fn get_job_logs(&self, job_id: &str) -> Result<Vec<JobLog>, CicdError> {
        let rows = sqlx::query_as::<_, JobLogRow>(
            "SELECT * FROM job_logs WHERE job_id = ? ORDER BY sequence ASC",
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to fetch job logs: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Count queued jobs
    pub async fn get_queued_count(&self) -> Result<i64, CicdError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status = 'queued'")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CicdError::DatabaseError(format!("Failed to count queued jobs: {}", e)))?;

        Ok(count.0)
    }

    /// Get the currently running job (if any)
    pub async fn get_current_job(&self) -> Result<Option<Job>, CicdError> {
        let row = sqlx::query_as::<_, JobRow>(
            r#"
            SELECT
                id, project_name, branch, status,
                commit_sha, commit_message, commit_author_name,
                started_at, completed_at, output, output_truncated, error
            FROM jobs
            WHERE status = 'running'
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to fetch current job: {}", e)))?;

        Ok(row.map(|r| r.into()))
    }

    /// Count completed jobs (success + failed)
    pub async fn get_completed_count(&self) -> Result<i64, CicdError> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status IN ('success', 'failed')")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    CicdError::DatabaseError(format!("Failed to count completed jobs: {}", e))
                })?;

        Ok(count.0)
    }

    /// Get jobs by status
    pub async fn get_jobs_by_status(
        &self,
        status: JobStatus,
        limit: i64,
    ) -> Result<Vec<Job>, CicdError> {
        let status_str = match status {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Success => "success",
            JobStatus::Failed => "failed",
        };

        let rows = sqlx::query_as::<_, JobRow>(
            r#"
            SELECT
                id, project_name, branch, status,
                commit_sha, commit_message, commit_author_name,
                started_at, completed_at, output, output_truncated, error
            FROM jobs
            WHERE status = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(status_str)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to fetch jobs by status: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get jobs by project and branch
    pub async fn get_jobs_by_branch(
        &self,
        project: &str,
        branch: &str,
        limit: i64,
    ) -> Result<Vec<Job>, CicdError> {
        let rows = sqlx::query_as::<_, JobRow>(
            r#"
            SELECT
                id, project_name, branch, status,
                commit_sha, commit_message, commit_author_name,
                started_at, completed_at, output, output_truncated, error
            FROM jobs
            WHERE project_name = ? AND branch = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(project)
        .bind(branch)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to fetch jobs by branch: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Get jobs by branch only (across all projects)
    pub async fn get_jobs_by_branch_only(
        &self,
        branch: &str,
        limit: i64,
    ) -> Result<Vec<Job>, CicdError> {
        let rows = sqlx::query_as::<_, JobRow>(
            r#"
            SELECT
                id, project_name, branch, status,
                commit_sha, commit_message, commit_author_name,
                started_at, completed_at, output, output_truncated, error
            FROM jobs
            WHERE branch = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(branch)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CicdError::DatabaseError(format!("Failed to fetch jobs by branch: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }
}

// Helper struct to map DB row to Job struct
#[derive(FromRow)]
struct JobRow {
    id: String,
    project_name: String,
    branch: String,
    status: String,
    commit_sha: Option<String>,
    commit_message: Option<String>,
    commit_author_name: Option<String>,
    started_at: String,
    completed_at: Option<String>,
    output: Option<String>,
    output_truncated: Option<bool>,
    error: Option<String>,
}

impl From<JobRow> for Job {
    fn from(row: JobRow) -> Self {
        let status = match row.status.as_str() {
            "queued" => JobStatus::Queued,
            "running" => JobStatus::Running,
            "success" => JobStatus::Success,
            "failed" => JobStatus::Failed,
            _ => JobStatus::Failed, // Default fallback
        };

        // Parse RFC 3339 datetime strings
        let started_at = DateTime::parse_from_rfc3339(&row.started_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let completed_at = row.completed_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        Job {
            id: row.id,
            project_name: row.project_name,
            branch: row.branch,
            commit_sha: row.commit_sha,
            commit_message: row.commit_message,
            commit_author: row.commit_author_name,
            status,
            started_at,
            completed_at,
            output: row.output,
            output_truncated: row.output_truncated.unwrap_or(false),
            error: row.error,
        }
    }
}
