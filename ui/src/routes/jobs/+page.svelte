<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { jobStream } from '$lib/api/sse';
	import type { Job, ProjectSummary } from '$lib/api/types';
	import JobCard from '$lib/components/JobCard.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import * as Select from '$lib/components/ui/select';
	import * as Empty from '$lib/components/ui/empty';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { RefreshCw, ChevronLeft, ChevronRight, Filter, Inbox } from '@lucide/svelte';

	let jobs = $state<Job[]>([]);
	let projects = $state<ProjectSummary[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Filter state
	let selectedProject = $state<string>('');
	let selectedStatus = $state<string>('');
	let branchFilter = $state<string>('');

	// Pagination state
	const limit = 20;
	let offset = $state(0);
	let currentPage = $derived(Math.floor(offset / limit) + 1);
	let totalPages = $derived(Math.ceil(total / limit));

	const statusOptions = [
		{ value: '', label: 'All Statuses' },
		{ value: 'queued', label: 'Queued' },
		{ value: 'running', label: 'Running' },
		{ value: 'success', label: 'Success' },
		{ value: 'failed', label: 'Failed' }
	];

	async function loadProjects() {
		try {
			const data = await api.getProjects();
			projects = data.projects;
		} catch (e) {
			console.error('Failed to load projects:', e);
		}
	}

	async function loadJobs() {
		loading = true;
		try {
			const params: any = { limit, offset };
			if (selectedProject) params.project = selectedProject;
			if (selectedStatus) params.status = selectedStatus;
			if (branchFilter) params.branch = branchFilter;

			const data = await api.getJobs(params);
			jobs = data.jobs;
			total = data.total;
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	function applyFilters() {
		offset = 0; // Reset to first page when filters change
		loadJobs();
		updateURL();
	}

	function clearFilters() {
		selectedProject = '';
		selectedStatus = '';
		branchFilter = '';
		offset = 0;
		loadJobs();
		updateURL();
	}

	function nextPage() {
		if (offset + limit < total) {
			offset += limit;
			loadJobs();
			updateURL();
		}
	}

	function prevPage() {
		if (offset >= limit) {
			offset -= limit;
			loadJobs();
			updateURL();
		}
	}

	function updateURL() {
		const params = new URLSearchParams();
		if (selectedProject) params.set('project', selectedProject);
		if (selectedStatus) params.set('status', selectedStatus);
		if (branchFilter) params.set('branch', branchFilter);
		if (offset > 0) params.set('offset', offset.toString());

		const query = params.toString();
		goto(`/jobs${query ? `?${query}` : ''}`, { replaceState: true, keepFocus: true });
	}

	function loadFromURL() {
		const params = new URLSearchParams(page.url.search);
		selectedProject = params.get('project') || '';
		selectedStatus = params.get('status') || '';
		branchFilter = params.get('branch') || '';
		offset = parseInt(params.get('offset') || '0', 10);
	}

	let lastEvent = $state<any>(null);
	let streamConnected = $state(false);

	// Subscribe to SSE streams
	$effect(() => {
		const unsubscribeEvent = jobStream.lastEvent.subscribe((event) => {
			lastEvent = event;
		});
		const unsubscribeConnected = jobStream.connected.subscribe((connected) => {
			streamConnected = connected;
		});

		return () => {
			unsubscribeEvent();
			unsubscribeConnected();
		};
	});

	// Initialize on mount
	$effect(() => {
		loadFromURL();
		loadProjects();
		loadJobs();
		jobStream.connect();
	});

	// React to SSE events
	$effect(() => {
		if (lastEvent) {
			loadJobs();
		}
	});

	let projectOptions = $derived([
		{ value: '', label: 'All Projects' },
		...projects.map((p) => ({ value: p.name, label: p.name }))
	]);
</script>

<div class="space-y-6">
	<div class="flex items-center justify-between">
		<h1 class="text-3xl font-bold tracking-tight">Jobs</h1>
		<Button variant="outline" size="icon" onclick={loadJobs} disabled={loading}>
			<RefreshCw class={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
		</Button>
	</div>

	{#if error}
		<div class="rounded-md bg-destructive/15 p-4 text-destructive">
			<p class="font-semibold">Error loading jobs</p>
			<p>{error}</p>
		</div>
	{/if}

	<!-- Filter Bar (disabled for now) -->
	<div class="pointer-events-none rounded-lg border bg-card p-4 opacity-50">
		<div class="mb-3 flex items-center gap-2">
			<Filter class="h-4 w-4 text-muted-foreground" />
			<h2 class="text-sm font-semibold">Filters (Coming Soon)</h2>
		</div>

		<div class="grid gap-4 md:grid-cols-4">
			<div class="space-y-2">
				<Label for="project-filter">Project</Label>
				<Select.Root type="single" bind:value={selectedProject} onValueChange={applyFilters}>
					<Select.Trigger id="project-filter">
						{projectOptions.find((p) => p.value === selectedProject)?.label || 'All Projects'}
					</Select.Trigger>
					<Select.Content>
						{#each projectOptions as option}
							<Select.Item value={option.value} label={option.label} />
						{/each}
					</Select.Content>
				</Select.Root>
			</div>

			<div class="space-y-2">
				<Label for="status-filter">Status</Label>
				<Select.Root type="single" bind:value={selectedStatus} onValueChange={applyFilters}>
					<Select.Trigger id="status-filter">
						{statusOptions.find((s) => s.value === selectedStatus)?.label || 'All Statuses'}
					</Select.Trigger>
					<Select.Content>
						{#each statusOptions as option}
							<Select.Item value={option.value} label={option.label} />
						{/each}
					</Select.Content>
				</Select.Root>
			</div>

			<div class="space-y-2">
				<Label for="branch-filter">Branch</Label>
				<Input
					id="branch-filter"
					type="text"
					placeholder="Filter by branch..."
					bind:value={branchFilter}
					onkeydown={(e) => e.key === 'Enter' && applyFilters()}
				/>
			</div>

			<div class="flex items-end gap-2">
				<Button onclick={applyFilters} class="flex-1">Apply</Button>
				<Button variant="outline" onclick={clearFilters}>Clear</Button>
			</div>
		</div>
	</div>

	<!-- Results Summary -->
	<div class="flex items-center justify-between text-sm text-muted-foreground">
		<div>
			Showing {jobs.length > 0 ? offset + 1 : 0} - {Math.min(offset + limit, total)} of {total}
			{total === 1 ? 'job' : 'jobs'}
		</div>
		{#if totalPages > 1}
			<div>
				Page {currentPage} of {totalPages}
			</div>
		{/if}
	</div>

	<!-- Jobs List -->
	{#if loading && jobs.length === 0}
		<div class="space-y-4">
			<Skeleton class="h-18 w-full" />
			<Skeleton class="h-18 w-full" />
			<Skeleton class="h-18 w-full" />
		</div>
	{:else if jobs.length === 0}
		<Empty.Root class="h-64 border">
			<Empty.Content>
				<Empty.Media>
					<Inbox class="h-16 w-16 opacity-50" />
				</Empty.Media>
				<Empty.Header>
					<Empty.Title>No jobs found</Empty.Title>
					<Empty.Description>Try adjusting your filters</Empty.Description>
				</Empty.Header>
			</Empty.Content>
		</Empty.Root>
	{:else}
		<div class="grid gap-4">
			{#each jobs as job (job.id)}
				<JobCard {job} />
			{/each}
		</div>
	{/if}

	<!-- Pagination -->
	{#if totalPages > 1}
		<div class="flex items-center justify-center gap-2">
			<Button variant="outline" onclick={prevPage} disabled={offset === 0}>
				<ChevronLeft class="mr-2 h-4 w-4" />
				Previous
			</Button>
			<div class="flex items-center gap-1 text-sm">
				<span class="font-medium">{currentPage}</span>
				<span class="text-muted-foreground">of</span>
				<span class="font-medium">{totalPages}</span>
			</div>
			<Button variant="outline" onclick={nextPage} disabled={offset + limit >= total}>
				Next
				<ChevronRight class="ml-2 h-4 w-4" />
			</Button>
		</div>
	{/if}
</div>
