<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { jobStream } from '$lib/api/sse';
	import type { StatsResponse, Job } from '$lib/api/types';
	import JobCard from '$lib/components/JobCard.svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { RefreshCw, Activity, Server, GitBranch } from '@lucide/svelte';

	let stats = $state<StatsResponse | null>(null);
	let recentJobs = $state<Job[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	async function loadData() {
		try {
			const [statsData, jobsData] = await Promise.all([api.getStats(), api.getJobs({ limit: 10 })]);
			stats = statsData;
			recentJobs = jobsData.jobs;
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadData();
		jobStream.connect();
	});

	// React to SSE events
	$effect(() => {
		if (jobStream.lastEvent) {
			// Refresh data on any job event to keep stats and list in sync
			loadData();
		}
	});

	function formatUptime(seconds: number): string {
		const days = Math.floor(seconds / 86400);
		const hours = Math.floor((seconds % 86400) / 3600);
		const mins = Math.floor((seconds % 3600) / 60);

		if (days > 0) return `${days}d ${hours}h`;
		if (hours > 0) return `${hours}h ${mins}m`;
		return `${mins}m`;
	}
</script>

<div class="space-y-8">
	<div class="flex items-center justify-between">
		<h1 class="text-3xl font-bold tracking-tight">Dashboard</h1>
		<div class="flex items-center gap-2">
			{#if jobStream.connected}
				<span class="flex h-2 w-2 animate-pulse rounded-full bg-green-500"></span>
				<span class="text-xs text-muted-foreground">Live</span>
			{:else}
				<span class="flex h-2 w-2 rounded-full bg-red-500"></span>
				<span class="text-xs text-muted-foreground">Offline</span>
			{/if}
			<Button variant="outline" size="icon" onclick={loadData} disabled={loading}>
				<RefreshCw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
			</Button>
		</div>
	</div>

	{#if error}
		<div class="rounded-md bg-destructive/15 p-4 text-destructive">
			<p class="font-semibold">Error loading dashboard</p>
			<p>{error}</p>
		</div>
	{/if}

	<!-- Stats Grid -->
	<div class="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
		<Card>
			<CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
				<CardTitle class="text-sm font-medium">Total Jobs</CardTitle>
				<Activity class="h-4 w-4 text-muted-foreground" />
			</CardHeader>
			<CardContent>
				{#if loading && !stats}
					<Skeleton class="h-8 w-20" />
				{:else}
					<div class="text-2xl font-bold">{stats?.jobs.total ?? 0}</div>
					<p class="text-xs text-muted-foreground">
						{stats?.jobs.success_rate.toFixed(1)}% success rate
					</p>
				{/if}
			</CardContent>
		</Card>

		<Card>
			<CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
				<CardTitle class="text-sm font-medium">Active Projects</CardTitle>
				<GitBranch class="h-4 w-4 text-muted-foreground" />
			</CardHeader>
			<CardContent>
				{#if loading && !stats}
					<Skeleton class="h-8 w-20" />
				{:else}
					<div class="text-2xl font-bold">{stats?.server.total_projects ?? 0}</div>
					<p class="text-xs text-muted-foreground">Configured repositories</p>
				{/if}
			</CardContent>
		</Card>

		<Card>
			<CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
				<CardTitle class="text-sm font-medium">Server Status</CardTitle>
				<Server class="h-4 w-4 text-muted-foreground" />
			</CardHeader>
			<CardContent>
				{#if loading && !stats}
					<Skeleton class="h-8 w-20" />
				{:else}
					<div class="text-2xl font-bold">Online</div>
					<p class="text-xs text-muted-foreground">
						Uptime: {formatUptime(stats?.server.uptime_seconds ?? 0)}
					</p>
				{/if}
			</CardContent>
		</Card>

		<Card>
			<CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
				<CardTitle class="text-sm font-medium">Queue Status</CardTitle>
				<RefreshCw class="h-4 w-4 text-muted-foreground" />
			</CardHeader>
			<CardContent>
				{#if loading && !stats}
					<Skeleton class="h-8 w-20" />
				{:else}
					<div class="text-2xl font-bold">
						{stats?.jobs.running ?? 0}
						<span class="text-sm font-normal text-muted-foreground">running</span>
					</div>
					<p class="text-xs text-muted-foreground">
						{stats?.jobs.queued ?? 0} queued
					</p>
				{/if}
			</CardContent>
		</Card>
	</div>

	<div class="grid gap-8 md:grid-cols-1">
		<div class="space-y-4">
			<h2 class="text-xl font-semibold tracking-tight">Recent Jobs</h2>

			{#if loading && recentJobs.length === 0}
				<div class="space-y-4">
					<Skeleton class="h-24 w-full" />
					<Skeleton class="h-24 w-full" />
					<Skeleton class="h-24 w-full" />
				</div>
			{:else if recentJobs.length === 0}
				<div
					class="flex h-32 items-center justify-center rounded-md border border-dashed text-muted-foreground"
				>
					No jobs found
				</div>
			{:else}
				<div class="grid gap-4">
					{#each recentJobs as job (job.id)}
						<JobCard {job} />
					{/each}
				</div>
			{/if}
		</div>
	</div>
</div>
