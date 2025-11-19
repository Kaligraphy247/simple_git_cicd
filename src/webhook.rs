//! Webhook related structures

/// Data extracted from webhook payload and configuration
/// This data is passed to scripts as environment variables
#[derive(Debug, Clone)]
pub struct WebhookData {
    pub project_name: String,
    pub branch: String,
    pub repo_path: String,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub commit_author_name: Option<String>,
    pub commit_author_email: Option<String>,
    pub pusher_name: Option<String>,
    pub repository_url: Option<String>,
}

impl WebhookData {
    /// Create minimal webhook data (when payload parsing fails or for testing)
    pub fn minimal(project_name: String, branch: String, repo_path: String) -> Self {
        Self {
            project_name,
            branch,
            repo_path,
            commit_sha: None,
            commit_message: None,
            commit_author_name: None,
            commit_author_email: None,
            pusher_name: None,
            repository_url: None,
        }
    }
}
