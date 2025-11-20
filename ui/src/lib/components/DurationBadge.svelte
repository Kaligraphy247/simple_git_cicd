<script lang="ts">
	import { Badge } from '$lib/components/ui/badge';
	import { cn, formatDuration } from '$lib/utils';
	import { Clock } from '@lucide/svelte';

	let { durationMs, class: className }: { durationMs: number | undefined; class?: string } =
		$props();
</script>

<Badge variant="outline" class={cn('gap-1.5 font-mono font-normal', className)}>
	<Clock class="h-3 w-3 opacity-70" />
	{formatDuration(durationMs)}
</Badge>
