export type JobStatus = 'queued' | 'running' | 'success' | 'failed';

export interface Job {
	id: string;
	project_name: string;
	branch: string;
	commit_sha?: string;
	commit_message?: string;
	commit_author?: string;
	status: JobStatus;
	started_at: string;
	completed_at?: string;
	output?: string;
	output_truncated: boolean;
	error?: string;
}

export interface JobLog {
	sequence: number;
	log_type: string;
	command?: string;
	started_at: string;
	completed_at?: string;
	duration_ms?: number;
	exit_code?: number;
	output?: string;
	status: string;
}

export interface JobsResponse {
	jobs: Job[];
	total: number;
	limit: number;
	offset: number;
}

export interface ServerStats {
	name: string;
	version: string;
	uptime_seconds: number;
	started_at: string;
	total_projects: number;
}

export interface JobStats {
	total: number;
	queued: number;
	running: number;
	success: number;
	failed: number;
	success_rate: number;
}

export interface StatsResponse {
	server: ServerStats;
	jobs: JobStats;
}

export interface ProjectSummary {
	name: string;
	branches: string[];
	last_job_status?: string;
	last_job_at?: string;
	success_rate: number;
	total_jobs: number;
}

export interface ProjectsResponse {
	projects: ProjectSummary[];
	count: number;
}

export interface ConfigResponse {
	config_toml: string;
	path: string;
}

export interface JobEvent {
	event_type: string;
	job_id: string;
	project_name: string;
	branch: string;
	timestamp: string;
}

export interface LogChunkEvent {
	job_id: string;
	step_type: string;
	chunk: string;
	timestamp: string;
}
