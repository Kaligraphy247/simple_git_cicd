<script lang="ts">
	import './layout.css';
	import { page } from '$app/state';
	import { theme } from '$lib/stores/theme';
	import { Button } from '$lib/components/ui/button';
	import { Separator } from '@/components/ui/separator';
	import { Toaster } from 'svelte-sonner';
	import * as Sheet from '$lib/components/ui/sheet';
	import {
		Sun,
		Moon,
		Menu,
		Server,
		LayoutDashboard,
		List,
		FolderGit,
		Settings
	} from '@lucide/svelte';

	const navItems = [
		{ href: '/', label: 'Dashboard', icon: LayoutDashboard },
		{ href: '/jobs', label: 'Jobs', icon: List },
		{ href: '/projects', label: 'Projects', icon: FolderGit },
		{ href: '/config', label: 'Config', icon: Settings }
	];

	let { children } = $props();
	let open: boolean = $state(false);
	let pathname: string = $derived(page.url.pathname);
	let titleTag: string = $derived.by(() => {
		for (const item of navItems) {
			if (item.href.startsWith(pathname)) {
				return `${item.label} - Simple Git CI/CD`;
			}
		}
		return 'Simple Git CI/CD';
	});

	function isActive(href: string, currentPath: string) {
		if (href === '/') return currentPath === '/';
		return currentPath.startsWith(href);
	}
</script>

<svelte:head>
	<title>{titleTag}</title>
</svelte:head>

<div class="mx-auto min-h-screen max-w-7xl bg-background font-sans antialiased">
	<!-- Toaster (Sonner Svelte) -->
	<Toaster theme={'system'} richColors={true} />

	<header
		class="sticky top-0 z-40 w-full border-b bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/60"
	>
		<div class="container mx-auto flex h-14 items-center px-4">
			<!-- Mobile Nav -->
			{@render MobileNav()}
			<a href="/" onclick={() => (open = false)} class="font-bold md:hidden"> Simple Git CI/CD </a>

			<!-- Desktop Nav -->
			{@render DesktopNav()}

			<!-- Right Side -->
			<div class="flex flex-1 items-center justify-between space-x-2 md:justify-end">
				<div class="w-full flex-1 md:w-auto md:flex-none"></div>
				<Button variant="ghost" size="icon" onclick={theme.toggle}>
					<Sun
						class="h-[1.2rem] w-[1.2rem] scale-100 rotate-0 transition-all dark:scale-0 dark:-rotate-90"
					/>
					<Moon
						class="absolute h-[1.2rem] w-[1.2rem] scale-0 rotate-90 transition-all dark:scale-100 dark:rotate-0"
					/>
					<span class="sr-only">Toggle theme</span>
				</Button>
			</div>
		</div>
	</header>

	<main class="container mx-auto px-4 py-6">
		{@render children()}
	</main>
</div>

{#snippet MobileNav()}
	<div class="mr-0 md:hidden">
		<Sheet.Root bind:open>
			<Sheet.Trigger>
				{#snippet child({ props })}
					<Button
						variant="ghost"
						size="icon"
						class="mr-2 px-0 text-base hover:bg-transparent focus-visible:bg-transparent focus-visible:ring-0 focus-visible:ring-offset-0 md:hidden"
						{...props}
					>
						<Menu class="h-5 w-5" />
						<span class="sr-only">Toggle Menu</span>
					</Button>
				{/snippet}
			</Sheet.Trigger>
			<Sheet.Content side="left" class="pr-0">
				<Sheet.Header>
					<Sheet.Title>
						<a href="/" onclick={() => (open = false)} class="mr-6 flex items-center space-x-2">
							<Server class="h-6 w-6" />
							<span class="font-bold">Simple Git CI/CD</span>
						</a>
					</Sheet.Title>
				</Sheet.Header>
				<Separator />
				<div class="ml-4 flex flex-col space-y-3">
					{#each navItems as item}
						<a
							href={item.href}
							onclick={() => (open = false)}
							class={isActive(item.href, page.url.pathname)
								? 'text-foreground'
								: 'text-foreground/60 transition-colors hover:text-foreground/80'}
						>
							<div class="flex items-center gap-2">
								<item.icon class="h-4 w-4" />
								{item.label}
							</div>
						</a>
					{/each}
				</div>
			</Sheet.Content>
		</Sheet.Root>
	</div>
{/snippet}

{#snippet DesktopNav()}
	<div class="mr-4 hidden md:flex">
		<a href="/" class="mr-6 flex items-center space-x-2">
			<Server class="h-6 w-6" />
			<span class="hidden font-bold sm:inline-block">Simple Git CI/CD</span>
		</a>
		<nav class="flex items-center space-x-6 text-sm font-medium">
			{#each navItems as item}
				<a
					href={item.href}
					class={isActive(item.href, page.url.pathname)
						? 'text-foreground'
						: 'text-foreground/60 transition-colors hover:text-foreground/80'}
				>
					{item.label}
				</a>
			{/each}
		</nav>
	</div>
{/snippet}
