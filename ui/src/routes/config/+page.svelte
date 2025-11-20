<script lang="ts">
	import { api } from '$lib/api/client';
	import type { ConfigResponse } from '$lib/api/types';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { ScrollArea } from '$lib/components/ui/scroll-area';
	import * as Alert from '$lib/components/ui/alert';
	import { RefreshCw, FileText, RotateCcw, CheckCircle2, AlertCircle } from '@lucide/svelte';

	let config = $state<ConfigResponse | null>(null);
	let loading = $state(true);
	let reloading = $state(false);
	let error = $state<string | null>(null);
	let reloadStatus = $state<{ type: 'success' | 'error'; message: string } | null>(null);

	async function loadConfig() {
		loading = true;
		try {
			config = await api.getConfig();
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function handleReload() {
		reloading = true;
		reloadStatus = null;
		try {
			const result = await api.reloadConfig();
			reloadStatus = {
				type: 'success',
				message: result.message || 'Configuration reloaded successfully'
			};
			// Reload the config display after successful reload
			await loadConfig();
		} catch (e) {
			reloadStatus = {
				type: 'error',
				message: e instanceof Error ? e.message : String(e)
			};
		} finally {
			reloading = false;
			// Clear status after 5 seconds
			setTimeout(() => {
				reloadStatus = null;
			}, 5000);
		}
	}

	$effect(() => {
		loadConfig();
	});
</script>

<div class="space-y-6">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-3xl font-bold tracking-tight">Configuration</h1>
			<p class="text-muted-foreground">Current TOML configuration file</p>
		</div>
		<div class="flex gap-2">
			<Button variant="outline" size="icon" onclick={loadConfig} disabled={loading}>
				<RefreshCw class={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
			</Button>
			<Button onclick={handleReload} disabled={reloading}>
				<RotateCcw class={`mr-2 h-4 w-4 ${reloading ? 'animate-spin' : ''}`} />
				Reload Config
			</Button>
		</div>
	</div>

	{#if error}
		<div class="rounded-md bg-destructive/15 p-4 text-destructive">
			<p class="font-semibold">Error loading configuration</p>
			<p>{error}</p>
		</div>
	{/if}

	{#if reloadStatus}
		{#if reloadStatus.type === 'success'}
			<Alert.Root variant="default" class="border-green-600/50 bg-green-50 dark:bg-green-950/20">
				<CheckCircle2 class="h-4 w-4 text-green-600" />
				<Alert.Title>Success</Alert.Title>
				<Alert.Description>{reloadStatus.message}</Alert.Description>
			</Alert.Root>
		{:else}
			<Alert.Root variant="destructive">
				<AlertCircle class="h-4 w-4" />
				<Alert.Title>Error</Alert.Title>
				<Alert.Description>{reloadStatus.message}</Alert.Description>
			</Alert.Root>
		{/if}
	{/if}

	{#if loading && !config}
		<Skeleton class="h-96 w-full" />
	{:else if config}
		<Card.Root>
			<Card.Header>
				<div class="flex items-center justify-between">
					<div>
						<Card.Title>Configuration File</Card.Title>
						<Card.Description>Read-only view of {config.path}</Card.Description>
					</div>
					<div class="flex items-center gap-2 text-sm text-muted-foreground">
						<FileText class="h-4 w-4" />
						<span class="font-mono text-xs">{config.config_toml.split('\n').length} lines</span>
					</div>
				</div>
			</Card.Header>
			<Card.Content>
				<ScrollArea class="h-[600px] w-full rounded border">
					<div class="bg-slate-950 p-4">
						<pre
							class="font-mono text-sm text-slate-50"><code>{config.config_toml}</code></pre>
					</div>
				</ScrollArea>
			</Card.Content>
			<Card.Footer class="flex-col items-start gap-2">
				<div class="text-sm text-muted-foreground">
					<p>
						To edit the configuration, modify the TOML file directly at <code
							class="rounded bg-muted px-1 py-0.5">{config.path}</code
						>.
					</p>
					<p class="mt-1">After making changes, click "Reload Config" to apply them.</p>
				</div>
			</Card.Footer>
		</Card.Root>

		<!-- Help Section -->
		<Card.Root>
			<Card.Header>
				<Card.Title>Configuration Guide</Card.Title>
			</Card.Header>
			<Card.Content class="space-y-3 text-sm">
				<div>
					<h3 class="font-semibold">Project Configuration</h3>
					<p class="text-muted-foreground">
						Each <code class="rounded bg-muted px-1 py-0.5">[[project]]</code> block defines a repository
						to watch.
					</p>
				</div>
				<div>
					<h3 class="font-semibold">Required Fields</h3>
					<ul class="ml-4 list-disc text-muted-foreground">
						<li><code class="rounded bg-muted px-1 py-0.5">name</code> - Repository name (must match GitHub webhook)</li>
						<li><code class="rounded bg-muted px-1 py-0.5">repo_path</code> - Absolute path to local repository</li>
						<li><code class="rounded bg-muted px-1 py-0.5">branches</code> - Array of branches to watch</li>
						<li><code class="rounded bg-muted px-1 py-0.5">run_script</code> - Script to execute on push</li>
					</ul>
				</div>
				<div>
					<h3 class="font-semibold">Optional Fields</h3>
					<ul class="ml-4 list-disc text-muted-foreground">
						<li><code class="rounded bg-muted px-1 py-0.5">with_webhook_secret</code> - Enable HMAC signature validation</li>
						<li><code class="rounded bg-muted px-1 py-0.5">webhook_secret</code> - Secret for signature validation</li>
						<li><code class="rounded bg-muted px-1 py-0.5">branch_scripts</code> - Branch-specific script overrides</li>
					</ul>
				</div>
			</Card.Content>
		</Card.Root>
	{/if}
</div>
