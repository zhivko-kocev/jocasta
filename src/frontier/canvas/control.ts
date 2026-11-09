/// <reference lib="dom" />
// The Desktop's slot on whiteboard's server (0393, 0552): `GET /control` streams `desktop-verb` frames as server-sent
// events; while the page holds the slot, an agent's `open` shows its review on this canvas.
import type { WhiteboardConnection } from "./contract.ts";
import { failure } from "./source.ts";

/** How long the page waits before taking the slot again after a drop or a refusal. */
const REATTACH_MS = 3_000;

/**
 * Each server-sent event's data in a byte stream: frames end at a blank line (LF or CRLF), may arrive split across
 * chunks, and a frame's `data:` lines join with newlines; frames with no data (comments, keep-alives) are skipped,
 * as is an unfinished frame when the stream ends.
 */
export async function* sseFrames(stream: ReadableStream<Uint8Array>): AsyncGenerator<string> {
	const reader = stream.getReader();
	const decoder = new TextDecoder();
	let buffered = "";
	try {
		for (;;) {
			const { value, done } = await reader.read();
			if (done) return;
			buffered += decoder.decode(value, { stream: true });
			for (let end = buffered.search(/\r?\n\r?\n/); end >= 0; end = buffered.search(/\r?\n\r?\n/)) {
				const block = buffered.slice(0, end);
				buffered = buffered.slice(end).replace(/^\r?\n\r?\n/, "");
				const data = block
					.split(/\r?\n/)
					.filter((line) => line.startsWith("data:"))
					.map((line) => line.slice(5).replace(/^ /, ""))
					.join("\n");
				if (data) yield data;
			}
		}
	} finally {
		reader.releaseLock();
	}
}

/**
 * Holds the control slot until `signal` aborts, handing each frame's data to `answer`; reattaches after a drop or a
 * refusal (another Desktop holds the slot).
 */
export async function attachControl(
	connection: WhiteboardConnection,
	signal: AbortSignal,
	answer: (data: string) => Promise<void>,
): Promise<void> {
	while (!signal.aborted) {
		try {
			const url = new URL("/control", connection.url);
			url.searchParams.set("token", connection.token);
			const response = await fetch(url, { signal });
			if (!response.ok || !response.body) throw await failure(response);
			for await (const data of sseFrames(response.body)) await answer(data);
		} catch {
			// Dropped or refused: try again shortly.
		}
		if (!signal.aborted) {
			const { promise, resolve } = Promise.withResolvers<void>();
			setTimeout(resolve, REATTACH_MS);
			await promise;
		}
	}
}
