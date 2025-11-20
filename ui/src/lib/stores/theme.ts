import { browser } from '$app/environment';
import { writable } from 'svelte/store';

type Theme = 'light' | 'dark';

const defaultValue: Theme = 'dark';

const initialValue = browser
	? (localStorage.getItem('theme') as Theme) ??
	  (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
	: defaultValue;

const { subscribe, set, update } = writable<Theme>(initialValue);

if (browser) {
	subscribe((value) => {
		const root = document.documentElement;
		root.classList.remove('light', 'dark');
		root.classList.add(value);
		localStorage.setItem('theme', value);
	});
}

export const theme = {
	subscribe,
	set,
	toggle: () => update((t) => (t === 'dark' ? 'light' : 'dark'))
};
