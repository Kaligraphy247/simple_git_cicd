<script lang="ts">
	import { api } from '$lib/api/client';
	import type { ProjectSummary } from '$lib/api/types';
	import { formatRelativeTime } from '$lib/utils';
	import * as Card from '$lib/components/ui/card';
	import * as Empty from '$lib/components/ui/empty';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import StatusBadge from '$lib/components/StatusBadge.svelte';
	import {
		FolderGit,
		GitBranch,
		Activity,
		TrendingUp,
		RefreshCw,
		ArrowRight
	} from '@lucide/svelte';

	let projects = $state<ProjectSummary[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	async function loadProjects() {
		loading = true;
		try {
			const data = await api.getProjects();
			projects = data.projects;
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		loadProjects();
	});
</script>

<div class="space-y-6">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-3xl font-bold tracking-tight">Projects</h1>
			<p class="text-muted-foreground">Configured repositories and their status</p>
		</div>
		<Button variant="outline" size="icon" onclick={loadProjects} disabled={loading}>
			<RefreshCw class={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
		</Button>
	</div>

	{#if error}
		<div class="rounded-md bg-destructive/15 p-4 text-destructive">
			<p class="font-semibold">Error loading projects</p>
			<p>{error}</p>
		</div>
	{/if}

	{#if loading && projects.length === 0}
		<div class="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
			<Skeleton class="h-64 w-full" />
			<Skeleton class="h-64 w-full" />
			<Skeleton class="h-64 w-full" />
		</div>
	{:else if projects.length === 0}
		<Empty.Root class="h-64 border">
			<Empty.Content>
				<Empty.Media>
					<FolderGit class="h-16 w-16 opacity-50" />
				</Empty.Media>
				<Empty.Header>
					<Empty.Title>No projects configured</Empty.Title>
					<Empty.Description>Add projects to your cicd_config.toml file</Empty.Description>
				</Empty.Header>
			</Empty.Content>
		</Empty.Root>
	{:else}
		<div class="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
			{#each projects as project}
				<Card.Root class="transition-all hover:border-primary/50 hover:shadow-md">
					<Card.Header>
						<div class="flex items-start justify-between">
							<div class="flex items-center gap-3">
								<div class="rounded-lg bg-primary/10 p-2">
									<FolderGit class="h-5 w-5 text-primary" />
								</div>
								<div>
									<Card.Title class="text-lg">{project.name}</Card.Title>
								</div>
							</div>
							{#if project.last_job_status}
								<StatusBadge status={project.last_job_status} />
							{/if}
						</div>
					</Card.Header>

					<Card.Content class="space-y-4">
						<!-- Branches -->
						<div class="space-y-2">
							<div class="flex items-center gap-2 text-sm text-muted-foreground">
								<GitBranch class="h-4 w-4" />
								<span>Branches ({project.branches.length})</span>
							</div>
							<div class="flex flex-wrap gap-1.5">
								{#each project.branches as branch}
									<Badge variant="outline" class="font-mono text-xs">{branch}</Badge>
								{/each}
							</div>
						</div>

						<!-- Stats -->
						<div class="grid grid-cols-2 gap-4 rounded-lg border p-3">
							<div class="space-y-1">
								<div class="flex items-center gap-2 text-xs text-muted-foreground">
									<Activity class="h-3.5 w-3.5" />
									<span>Total Jobs</span>
								</div>
								<p class="text-xl font-bold">{project.total_jobs}</p>
							</div>
							<div class="space-y-1">
								<div class="flex items-center gap-2 text-xs text-muted-foreground">
									<TrendingUp class="h-3.5 w-3.5" />
									<span>Success Rate</span>
								</div>
								<p class="text-xl font-bold">{project.success_rate.toFixed(1)}%</p>
							</div>
						</div>

						<!-- Last Job -->
						{#if project.last_job_at}
							<div class="text-sm text-muted-foreground">
								Last job: {formatRelativeTime(project.last_job_at)}
							</div>
						{/if}
					</Card.Content>

					<Card.Footer>
						<Button
							variant="outline"
							class="w-full"
							href="/jobs?project={encodeURIComponent(project.name)}"
						>
							View Jobs
							<ArrowRight class="ml-2 h-4 w-4" />
						</Button>
					</Card.Footer>
				</Card.Root>
			{/each}
		</div>
	{/if}
</div>
