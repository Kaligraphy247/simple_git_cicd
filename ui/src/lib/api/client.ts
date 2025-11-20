import type {
	ConfigResponse,
	Job,
	JobLog,
	JobsResponse,
	ProjectsResponse,
	StatsResponse
} from './types';

const API_BASE = '/api';

async function fetchJson<T>(url: string, options?: RequestInit): Promise<T> {
	const res = await fetch(`${API_BASE}${url}`, options);
	if (!res.ok) {
		let errorMsg = res.statusText;
		try {
			const errorBody = await res.json();
			if (errorBody && errorBody.error) {
				errorMsg = errorBody.error;
			}
		} catch {
			// ignore json parse error if response is not json
		}
		throw new Error(`API Error ${res.status}: ${errorMsg}`);
	}
	return res.json();
}

export const api = {
	async getJobs(params?: {
		project?: string;
		branch?: string;
		status?: string;
		limit?: number;
		offset?: number;
	}): Promise<JobsResponse> {
		const query = new URLSearchParams();
		if (params) {
			if (params.project) query.append('project', params.project);
			if (params.branch) query.append('branch', params.branch);
			if (params.status) query.append('status', params.status);
			if (params.limit) query.append('limit', params.limit.toString());
			if (params.offset) query.append('offset', params.offset.toString());
		}
		const queryString = query.toString();
		return fetchJson<JobsResponse>(`/jobs${queryString ? `?${queryString}` : ''}`);
	},

	async getJob(id: string): Promise<Job> {
		return fetchJson<Job>(`/jobs/${id}`);
	},

	async getJobLogs(id: string): Promise<{ job_id: string; logs: JobLog[]; count: number }> {
		return fetchJson<{ job_id: string; logs: JobLog[]; count: number }>(`/jobs/${id}/logs`);
	},

	async getProjects(): Promise<ProjectsResponse> {
		return fetchJson<ProjectsResponse>('/projects');
	},

	async getStats(): Promise<StatsResponse> {
		return fetchJson<StatsResponse>('/stats');
	},

	async getConfig(): Promise<ConfigResponse> {
		return fetchJson<ConfigResponse>('/config/current');
	},

	async reloadConfig(): Promise<{ status: string; message: string }> {
		// Note: /reload is at the root, not under /api
		// This requires proxy setup in vite.config.ts to handle /reload as well
		const res = await fetch('/reload', { method: 'POST' });
		if (!res.ok) {
			let errorMsg = res.statusText;
			try {
				const errorBody = await res.json();
				if (errorBody && errorBody.error) {
					errorMsg = errorBody.error;
				}
			} catch {
				// ignore
			}
			throw new Error(`API Error ${res.status}: ${errorMsg}`);
		}
		return res.json();
	}
};
