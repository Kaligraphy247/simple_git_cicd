<script lang="ts">
	import { formatRelativeTime } from '$lib/utils';
	import { Card, CardContent } from '$lib/components/ui/card';
	import { Badge } from '$lib/components/ui/badge';
	import { GitCommitHorizontal, Calendar, FlaskConical } from '@lucide/svelte';
	import StatusBadge from '$lib/components/StatusBadge.svelte';
	import DurationBadge from '$lib/components/DurationBadge.svelte';
	import type { Job } from '$lib/api/types';

	let { job }: { job: Job } = $props();

	let duration = $derived.by(() => {
		if (job.completed_at && job.started_at) {
			return new Date(job.completed_at).getTime() - new Date(job.started_at).getTime();
		}
		return undefined;
	});
</script>

<a href="/jobs/{job.id}" class="group block">
	<Card class="transition-all hover:border-primary/50 hover:shadow-md">
		<CardContent class="flex items-center justify-between gap-4 px-4">
			<div class="flex min-w-0 items-center gap-4">
				<StatusBadge status={job.status} />

				<div class="flex min-w-0 flex-col gap-1">
					<div class="flex items-center gap-2 truncate font-medium">
						<span class="text-foreground">{job.project_name}</span>
						<span class="text-muted-foreground">/</span>
						<span class="text-foreground">{job.branch}</span>
						{#if job.dry_run}
							<Badge variant="outline" class="ml-1 gap-1 text-xs">
								<FlaskConical class="h-3 w-3" />
								DRY RUN
							</Badge>
						{/if}
					</div>

					<div class="flex items-center gap-3 text-sm text-muted-foreground">
						<div class="flex items-center gap-1">
							<GitCommitHorizontal class="h-3.5 w-3.5" />
							<span class="font-mono"
								>{job.commit_sha ? job.commit_sha.substring(0, 7) : 'no-sha'}</span
							>
						</div>
						<div class="flex items-center gap-1">
							<Calendar class="h-3.5 w-3.5" />
							<span>{formatRelativeTime(job.started_at)}</span>
						</div>
					</div>
				</div>
			</div>

			<div class="shrink-0">
				{#if duration !== undefined || job.status === 'running'}
					<DurationBadge durationMs={duration} />
				{/if}
			</div>
		</CardContent>
	</Card>
</a>
