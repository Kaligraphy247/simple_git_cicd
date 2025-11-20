<script lang="ts">
	import { Badge } from '$lib/components/ui/badge';
	import { cn } from '$lib/utils';
	import type { JobStatus } from '$lib/api/types';
	import { CircleCheck, XCircle, Clock, Loader } from '@lucide/svelte';

	let { status, class: className }: { status: JobStatus | string; class?: string } = $props();

	// normalize status to lowercase for matching
	let s = $derived(status.toLowerCase());

	let config = $derived.by(() => {
		switch (s) {
			case 'success':
				return {
					icon: CircleCheck,
					label: 'Success',
					classes: 'bg-green-600 hover:bg-green-700 border-transparent text-white'
				};
			case 'failed':
				return {
					icon: XCircle,
					label: 'Failed',
					classes: 'bg-red-600 hover:bg-red-700 border-transparent text-white'
				};
			case 'running':
				return {
					icon: Loader,
					label: 'Running',
					classes: 'bg-blue-600 hover:bg-blue-700 border-transparent text-white'
				};
			case 'queued':
				return {
					icon: Clock,
					label: 'Queued',
					classes: 'bg-yellow-500 hover:bg-yellow-600 border-transparent text-white'
				};
			default:
				return {
					icon: Clock,
					label: status,
					classes: 'bg-gray-500 hover:bg-gray-600 border-transparent text-white'
				};
		}
	});
</script>

<Badge class={cn('gap-1.5 px-2.5 py-0.5 transition-colors', config.classes, className)}>
	<config.icon class={cn('h-3.5 w-3.5', s === 'running' && 'animate-spin')} />
	{config.label}
</Badge>
