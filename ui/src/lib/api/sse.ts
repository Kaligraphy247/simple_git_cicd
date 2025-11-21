import { writable } from 'svelte/store';
import type { JobEvent, LogChunkEvent } from './types';

function createJobStream() {
	const { subscribe: subscribeConnected, set: setConnected } = writable(false);
	const { subscribe: subscribeLastEvent, set: setLastEvent } = writable<JobEvent | null>(null);

	let eventSource: EventSource | null = null;
	let reconnectTimeout: number | null = null;
	let retryCount = 0;
	const maxRetries = 10;

	function disconnect() {
		if (eventSource) {
			eventSource.close();
			eventSource = null;
		}
		setConnected(false);
		if (reconnectTimeout) {
			clearTimeout(reconnectTimeout);
			reconnectTimeout = null;
		}
	}

	function scheduleReconnect() {
		if (retryCount >= maxRetries) {
			console.error('Max SSE retries reached');
			return;
		}

		const delay = Math.min(1000 * Math.pow(2, retryCount), 30000);
		retryCount++;

		if (reconnectTimeout) clearTimeout(reconnectTimeout);
		reconnectTimeout = window.setTimeout(() => {
			connect();
		}, delay);
	}

	function connect() {
		// If already connected or connecting, skip
		if (eventSource?.readyState === EventSource.OPEN) return;

		// Clean up any partial connection
		if (eventSource) disconnect();

		eventSource = new EventSource('/api/stream/jobs');

		eventSource.onopen = () => {
			setConnected(true);
			retryCount = 0;
		};

		const handleEvent = (event: MessageEvent) => {
			try {
				const data = JSON.parse(event.data) as JobEvent;
				setLastEvent(data);
			} catch (e) {
				console.error('Failed to parse SSE event:', e);
			}
		};

		eventSource.onmessage = handleEvent;
		eventSource.addEventListener('created', handleEvent);
		eventSource.addEventListener('running', handleEvent);
		eventSource.addEventListener('success', handleEvent);
		eventSource.addEventListener('failed', handleEvent);

		eventSource.onerror = () => {
			// On error, close and attempt manual reconnect with backoff
			setConnected(false);
			eventSource?.close();
			eventSource = null;
			scheduleReconnect();
		};
	}

	return {
		connected: { subscribe: subscribeConnected },
		lastEvent: { subscribe: subscribeLastEvent },
		connect,
		disconnect
	};
}

export const jobStream = createJobStream();

function createLogStream() {
	const { subscribe: subscribeConnected, set: setConnected } = writable(false);
	const { subscribe: subscribeLastChunk, set: setLastChunk } = writable<LogChunkEvent | null>(null);

	let eventSource: EventSource | null = null;
	let reconnectTimeout: number | null = null;
	let retryCount = 0;
	const maxRetries = 10;

	function disconnect() {
		if (eventSource) {
			eventSource.close();
			eventSource = null;
		}
		setConnected(false);
		if (reconnectTimeout) {
			clearTimeout(reconnectTimeout);
			reconnectTimeout = null;
		}
	}

	function scheduleReconnect() {
		if (retryCount >= maxRetries) {
			console.error('Max log SSE retries reached');
			return;
		}

		const delay = Math.min(1000 * Math.pow(2, retryCount), 30000);
		retryCount++;

		if (reconnectTimeout) clearTimeout(reconnectTimeout);
		reconnectTimeout = window.setTimeout(() => {
			connect();
		}, delay);
	}

	function connect() {
		// If already connected or connecting, skip
		if (eventSource?.readyState === EventSource.OPEN) return;

		// Clean up any partial connection
		if (eventSource) disconnect();

		eventSource = new EventSource('/api/stream/logs');

		eventSource.onopen = () => {
			setConnected(true);
			retryCount = 0;
		};

		eventSource.addEventListener('log_chunk', (event: MessageEvent) => {
			try {
				const data = JSON.parse(event.data) as LogChunkEvent;
				setLastChunk(data);
			} catch (e) {
				console.error('Failed to parse log chunk:', e);
			}
		});

		eventSource.onerror = () => {
			// On error, close and attempt manual reconnect with backoff
			setConnected(false);
			eventSource?.close();
			eventSource = null;
			scheduleReconnect();
		};
	}

	return {
		connected: { subscribe: subscribeConnected },
		lastChunk: { subscribe: subscribeLastChunk },
		connect,
		disconnect
	};
}

export const logStream = createLogStream();
