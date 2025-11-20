-- Main jobs table
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,                    -- UUID v7
    project_name TEXT NOT NULL,
    branch TEXT NOT NULL,
    status TEXT NOT NULL,                   -- queued, running, success, failed

    -- Webhook data
    commit_sha TEXT,
    commit_message TEXT,
    commit_author_name TEXT,
    commit_author_email TEXT,
    pusher_name TEXT,
    repository_url TEXT,

    -- Timing
    started_at TEXT NOT NULL,               -- RFC 3339
    completed_at TEXT,
    duration_ms INTEGER,                    -- Computed: completed_at - started_at

    -- Output & errors
    output TEXT,                            -- Combined stdout/stderr
    output_truncated BOOLEAN DEFAULT 0,
    error TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_jobs_project ON jobs(project_name);
CREATE INDEX idx_jobs_branch ON jobs(branch);
CREATE INDEX idx_jobs_status ON jobs(status);
CREATE INDEX idx_jobs_started_at ON jobs(started_at DESC);
CREATE INDEX idx_jobs_project_branch ON jobs(project_name, branch);
CREATE INDEX idx_jobs_created_at ON jobs(created_at DESC);

-- Job execution logs (structured log entries for each step)
CREATE TABLE job_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,              -- Order within job
    log_type TEXT NOT NULL,                 -- git_fetch, git_reset, pre_script, main_script, post_success, post_failure, post_always
    command TEXT,                           -- The actual command run
    started_at TEXT NOT NULL,
    completed_at TEXT,
    duration_ms INTEGER,
    exit_code INTEGER,
    output TEXT,                            -- stdout + stderr for this step
    status TEXT NOT NULL,                   -- running, success, failed

    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

CREATE INDEX idx_job_logs_job_id ON job_logs(job_id, sequence);

-- Config snapshots (track config changes over time)
CREATE TABLE config_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_at TEXT NOT NULL DEFAULT (datetime('now')),
    config_toml TEXT NOT NULL,
    reason TEXT                             -- startup, reload, manual
);

CREATE INDEX idx_config_snapshots_at ON config_snapshots(snapshot_at DESC);
