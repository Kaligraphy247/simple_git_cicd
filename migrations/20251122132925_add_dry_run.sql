-- Add dry_run column to jobs table
ALTER TABLE jobs ADD COLUMN dry_run BOOLEAN NOT NULL DEFAULT 0;

-- Index for filtering dry run jobs
CREATE INDEX idx_jobs_dry_run ON jobs(dry_run);
