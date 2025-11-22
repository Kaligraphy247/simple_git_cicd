<script lang="ts">
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { jobStream, logStream } from '$lib/api/sse';
	import type { Job, JobLog, LogChunkEvent } from '$lib/api/types';
	import { toast } from 'svelte-sonner';
	import { formatDate, formatDuration } from '$lib/utils';
	import StatusBadge from '$lib/components/StatusBadge.svelte';
	import DurationBadge from '$lib/components/DurationBadge.svelte';
	import * as Breadcrumb from '$lib/components/ui/breadcrumb';
	import * as Card from '$lib/components/ui/card';
	import * as Tabs from '$lib/components/ui/tabs';
	import * as Empty from '$lib/components/ui/empty';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import Spinner from '@/components/ui/spinner/spinner.svelte';
	import { ScrollArea } from '$lib/components/ui/scroll-area';
	import {
		GitCommitHorizontal,
		User,
		Calendar,
		Clock,
		Copy,
		CircleCheck,
		X,
		Terminal,
		FileText,
		FlaskConical,
		SkipForward
	} from '@lucide/svelte';
	import { Badge } from '$lib/components/ui/badge';

	let job = $state<Job | null>(null);
	let logs = $state<JobLog[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let autoScroll = $state(true);
	let lastEvent = $state<any>(null);
	let liveOutput = $state<string>('');
	let lastChunk = $state<LogChunkEvent | null>(null);
	let logsLoading = $state(false);
	let pendingLogsRequest: Promise<void> | null = null;
	let lastProcessedChunkTimestamp = $state<string | null>(null);

	const jobId = $derived(page.params.id as string);

	async function loadJob() {
		const isInitialLoad = loading && !job;

		try {
			const jobData = await api.getJob(jobId);
			job = jobData;

			await loadJobLogs();

			// Reset live output when job is complete (use stored output)
			if (jobData.status !== 'running') {
				liveOutput = '';
			}
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			if (isInitialLoad) {
				loading = false;
			}
		}
	}

	async function loadJobLogs() {
		if (logsLoading && pendingLogsRequest) {
			return pendingLogsRequest;
		}

		logsLoading = true;

		const request = (async () => {
			try {
				const logsData = await api.getJobLogs(jobId);
				logs = logsData.logs;
			} catch (logErr) {
				if (job?.status === 'running') {
					logs = [];
				}
				console.warn(`Failed to fetch logs for ${jobId}:`, logErr);
			} finally {
				logsLoading = false;
				pendingLogsRequest = null;
			}
		})();

		pendingLogsRequest = request;
		return request;
	}

	async function copyToClipboard(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			toast.success('Copied');
		} catch (e) {
			console.error('Failed to copy:', e);
		}
	}

	// Subscribe to SSE streams
	$effect(() => {
		const unsubscribeEvent = jobStream.lastEvent.subscribe((event) => {
			lastEvent = event;
		});

		const unsubscribeChunk = logStream.lastChunk.subscribe((chunk) => {
			lastChunk = chunk;
		});

		return () => {
			unsubscribeEvent();
			unsubscribeChunk();
		};
	});

	// Initialize on mount and setup refresh interval
	$effect(() => {
		loadJob();
		jobStream.connect();
		logStream.connect();

		// Refresh every 2 seconds if job is running
		const refreshInterval = window.setInterval(() => {
			if (job?.status === 'running') {
				loadJob();
			}
		}, 2000);

		return () => {
			clearInterval(refreshInterval);
		};
	});

	// React to SSE events for this job
	$effect(() => {
		if (lastEvent && lastEvent.job_id === jobId) {
			loadJob();
		}
	});

	// React to log chunks for this job
	$effect(() => {
		if (
			lastChunk &&
			lastChunk.job_id === jobId &&
			lastChunk.timestamp !== lastProcessedChunkTimestamp
		) {
			lastProcessedChunkTimestamp = lastChunk.timestamp;
			liveOutput += lastChunk.chunk;
			// Also refresh logs to show updated timeline
			loadJobLogs();
		}
	});

	let duration = $derived(
		job?.completed_at && job?.started_at
			? new Date(job.completed_at).getTime() - new Date(job.started_at).getTime()
			: undefined
	);

	let isRunning = $derived(job?.status === 'running');

	// Show live output while running, stored output when complete
	let displayOutput = $derived(isRunning && liveOutput ? liveOutput : job?.output || '');

	function getStepIcon(status: string) {
		switch (status.toLowerCase()) {
			case 'success':
				return CircleCheck;
			case 'failed':
			case 'error':
				return X;
			case 'running':
				return Spinner;
			case 'skipped':
				return SkipForward;
			default:
				return Terminal;
		}
	}

	function getStepIconClass(status: string) {
		switch (status.toLowerCase()) {
			case 'success':
				return 'text-green-600';
			case 'failed':
			case 'error':
				return 'text-red-600';
			case 'running':
				return 'text-blue-600 animate-spin';
			case 'skipped':
				return 'text-yellow-600';
			default:
				return 'text-muted-foreground';
		}
	}
</script>

<div class="space-y-6">
	<!-- Breadcrumbs -->
	<Breadcrumb.Root>
		<Breadcrumb.List>
			<Breadcrumb.Item>
				<Breadcrumb.Link href="/">Home</Breadcrumb.Link>
			</Breadcrumb.Item>
			<Breadcrumb.Separator />
			<Breadcrumb.Item>
				<Breadcrumb.Link href="/jobs">Jobs</Breadcrumb.Link>
			</Breadcrumb.Item>
			<Breadcrumb.Separator />
			<Breadcrumb.Item>
				<Breadcrumb.Page>{jobId.substring(0, 8)}</Breadcrumb.Page>
			</Breadcrumb.Item>
		</Breadcrumb.List>
	</Breadcrumb.Root>

	{#if error}
		<div class="rounded-md bg-destructive/15 p-4 text-destructive">
			<p class="font-semibold">Error loading job</p>
			<p>{error}</p>
		</div>
	{/if}

	{#if loading && !job}
		<div class="space-y-4">
			<Skeleton class="h-32 w-full" />
			<Skeleton class="h-64 w-full" />
		</div>
	{:else if job}
		<!-- Header -->
		<Card.Root>
			<Card.Header>
				<div class="flex items-start justify-between">
					<div class="space-y-3">
						<div class="flex items-center gap-3">
							<StatusBadge status={job.status} class="text-base" />
							{#if job.dry_run}
								<Badge variant="outline" class="gap-1">
									<FlaskConical class="h-3.5 w-3.5" />
									DRY RUN
								</Badge>
							{/if}
							{#if isRunning}
								<span class="text-sm text-muted-foreground">Job in progress...</span>
							{/if}
						</div>

						<div class="flex items-center gap-2 text-2xl font-bold">
							<span>{job.project_name}</span>
							<span class="text-muted-foreground">/</span>
							<span class="text-primary">{job.branch}</span>
						</div>
					</div>

					<div class="text-right text-sm">
						<div class="flex items-center justify-end gap-2 text-muted-foreground">
							<Calendar class="h-4 w-4" />
							<span>Started: {formatDate(job.started_at)}</span>
						</div>
						{#if job.completed_at}
							<div class="mt-1 flex items-center justify-end gap-2 text-muted-foreground">
								<Clock class="h-4 w-4" />
								<span>Completed: {formatDate(job.completed_at)}</span>
							</div>
						{/if}
						{#if duration !== undefined}
							<div class="mt-2">
								<DurationBadge durationMs={duration} />
							</div>
						{/if}
					</div>
				</div>
			</Card.Header>
		</Card.Root>

		<!-- Commit Info -->
		{#if job.commit_sha}
			<Card.Root>
				<Card.Header>
					<Card.Title>Commit Information</Card.Title>
				</Card.Header>
				<Card.Content class="space-y-3">
					<div class="flex items-start gap-3">
						<GitCommitHorizontal class="mt-0.5 h-5 w-5 text-muted-foreground" />
						<div class="flex-1 space-y-1">
							<div class="flex items-center gap-2">
								<code class="rounded bg-muted px-2 py-1 font-mono text-sm"
									>{job.commit_sha.substring(0, 7)}</code
								>
								<Button
									variant="ghost"
									size="icon"
									class="h-6 w-6"
									onclick={() => copyToClipboard(job?.commit_sha || '')}
								>
									<Copy class="h-3 w-3" />
								</Button>
							</div>
							{#if job.commit_message}
								<pre
									class="wrap-break-words mt-1 text-sm whitespace-pre-wrap text-foreground">{job.commit_message}</pre>
							{/if}
						</div>
					</div>

					{#if job.commit_author}
						<div class="flex items-center gap-3">
							<User class="h-5 w-5 text-muted-foreground" />
							<span class="text-sm text-muted-foreground">{job.commit_author}</span>
						</div>
					{/if}
				</Card.Content>
			</Card.Root>
		{/if}

		<!-- Tabs -->
		<Tabs.Root value="timeline" class="w-full">
			<Tabs.List class="grid w-full grid-cols-4">
				<Tabs.Trigger value="timeline">Timeline</Tabs.Trigger>
				<Tabs.Trigger value="console">Console Output</Tabs.Trigger>
				<Tabs.Trigger value="environment">Environment</Tabs.Trigger>
				<Tabs.Trigger value="raw">Raw JSON</Tabs.Trigger>
			</Tabs.List>

			<!-- Timeline Tab -->
			<Tabs.Content value="timeline" class="space-y-4">
				<Card.Root>
					<Card.Header>
						<Card.Title>Execution Timeline</Card.Title>
						<Card.Description>Step-by-step execution log</Card.Description>
					</Card.Header>
					<Card.Content>
						{#if logs.length === 0}
							<Empty.Root class="border-0">
								<Empty.Content>
									<Empty.Media>
										<FileText class="h-12 w-12 opacity-50" />
									</Empty.Media>
									<Empty.Header>
										<Empty.Title>No execution steps recorded</Empty.Title>
									</Empty.Header>
								</Empty.Content>
							</Empty.Root>
						{:else}
							<div class="space-y-4">
								{#each logs as log, i}
									{@const IconComponent = getStepIcon(log.status)}
									<div class="flex gap-4">
										<div class="flex flex-col items-center">
											<div
												class="flex h-8 w-8 items-center justify-center rounded-full border-2 bg-background p-1.5"
											>
												{#if IconComponent}
													<IconComponent class="h-4 w-4 {getStepIconClass(log.status)}" />
												{/if}
											</div>
											{#if i < logs.length - 1}
												<div class="h-full w-0.5 bg-border"></div>
											{/if}
										</div>

										<div class="flex-1 pb-4">
											<div class="flex items-start justify-between">
												<div>
													<p class="font-medium">{log.log_type}</p>
													{#if log.command}
														<code class="mt-1 block text-xs text-muted-foreground"
															>{log.command}</code
														>
													{/if}
												</div>
												<div class="text-right text-xs text-muted-foreground">
													{#if log.duration_ms !== undefined}
														<span>{formatDuration(log.duration_ms)}</span>
													{/if}
												</div>
											</div>

											{#if log.output}
												<details class="mt-2">
													<summary
														class="cursor-pointer text-sm text-muted-foreground hover:text-foreground"
													>
														View output
													</summary>
													<ScrollArea class="mt-2 h-48 rounded border bg-muted/50 p-3">
														<pre class="text-xs"><code>{log.output}</code></pre>
													</ScrollArea>
												</details>
											{/if}

											{#if log.exit_code !== undefined && log.exit_code !== 0}
												<div class="mt-2 text-sm text-red-600">Exit code: {log.exit_code}</div>
											{/if}
										</div>
									</div>
								{/each}
							</div>
						{/if}
					</Card.Content>
				</Card.Root>
			</Tabs.Content>

			<!-- Console Output Tab -->
			<Tabs.Content value="console" class="h-[calc(100vh-28rem)]">
				<Card.Root class="flex h-full flex-col">
					<Card.Header class="shrink-0">
						<div class="flex items-center justify-between">
							<div>
								<Card.Title>Console Output</Card.Title>
								<Card.Description>Full execution log</Card.Description>
							</div>
							<div class="flex items-center gap-2">
								<label class="flex items-center gap-2 text-sm">
									<input type="checkbox" bind:checked={autoScroll} class="rounded" />
									Auto-scroll
								</label>
								{#if isRunning}
									<Spinner class="h-4 w-4 animate-spin text-blue-600" />
								{/if}
							</div>
						</div>
					</Card.Header>
					<Card.Content class="flex-1 overflow-hidden">
						<ScrollArea class="h-full w-full rounded border">
							<div class="bg-slate-950 p-4 font-mono text-sm text-slate-50">
								{#if displayOutput}
									<pre class="wrap-break-words whitespace-pre-wrap">{displayOutput}</pre>
								{:else}
									<p class="text-slate-400">No output available</p>
								{/if}
								{#if job.output_truncated}
									<p class="mt-4 text-yellow-400">
										[Output truncated - full logs available in database]
									</p>
								{/if}
							</div>
						</ScrollArea>
					</Card.Content>
				</Card.Root>
			</Tabs.Content>

			<!-- Environment Tab -->
			<Tabs.Content value="environment">
				<Card.Root>
					<Card.Header>
						<Card.Title>Environment Variables</Card.Title>
						<Card.Description>Variables passed to the job script</Card.Description>
					</Card.Header>
					<Card.Content>
						<div class="space-y-2 rounded border bg-muted/50 p-4 font-mono text-sm">
							<div class="flex justify-between">
								<span class="text-muted-foreground">PROJECT_NAME</span>
								<span>{job.project_name}</span>
							</div>
							<div class="flex justify-between">
								<span class="text-muted-foreground">BRANCH</span>
								<span>{job.branch}</span>
							</div>
							{#if job.commit_sha}
								<div class="flex justify-between">
									<span class="text-muted-foreground">COMMIT_SHA</span>
									<span>{job.commit_sha}</span>
								</div>
							{/if}
							{#if job.commit_author}
								<div class="flex justify-between">
									<span class="text-muted-foreground">COMMIT_AUTHOR</span>
									<span>{job.commit_author}</span>
								</div>
							{/if}
						</div>
					</Card.Content>
				</Card.Root>
			</Tabs.Content>

			<!-- Raw JSON Tab -->
			<Tabs.Content value="raw" class="h-[calc(100vh-28rem)]">
				<Card.Root class="flex h-full flex-col">
					<Card.Header class="shrink-0">
						<div class="flex items-center justify-between">
							<div>
								<Card.Title>Raw JSON</Card.Title>
								<Card.Description>Debug view of job data</Card.Description>
							</div>
							<Button
								variant="outline"
								size="sm"
								onclick={() => copyToClipboard(JSON.stringify({ job, logs }, null, 2))}
							>
								<Copy class="mr-2 h-4 w-4" />
								Copy
							</Button>
						</div>
					</Card.Header>
					<Card.Content class="flex-1 overflow-hidden">
						<ScrollArea class="h-full w-full">
							<pre class="rounded bg-muted p-4 text-xs"><code
									>{JSON.stringify({ job, logs }, null, 2)}</code
								></pre>
						</ScrollArea>
					</Card.Content>
				</Card.Root>
			</Tabs.Content>
		</Tabs.Root>
	{/if}
</div>
