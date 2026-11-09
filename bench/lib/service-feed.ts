import { appendFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { WS_PATH } from "../../src/frontier/protocol.ts";
import { jsonLines } from "../../src/shared/jsonl.ts";
import { readServiceFile } from "../../src/shared/service-file.ts";

/**
 * The service's feed, recorded (A5): the service's log holds warnings only, so toasts are taken from its state socket
 * (`/ws`, the token as a bearer), which broadcasts every snapshot, toast and HUD message; each message is appended
 * with its arrival time.
 */

export interface FeedEntry {
	/** Epoch ms when the message arrived. */
	at: number;
	message: { type?: string; reason?: string; level?: string; text?: string; paneId?: string; priority?: string };
}

export type Toast = FeedEntry & { message: { type: "toast" } };

export function parseFeed(text: string): FeedEntry[] {
	return jsonLines<FeedEntry>(text).filter(
		(entry) => typeof entry.at === "number" && typeof entry.message === "object",
	);
}

/** The toasts in a feed; `urgentOnly` keeps those the service also raises as a herdr notification (`blocked`, urgent). */
export function toastsOf(entries: readonly FeedEntry[], urgentOnly = false): Toast[] {
	return entries.filter(
		(entry): entry is Toast =>
			entry.message.type === "toast" &&
			(!urgentOnly || entry.message.reason === "blocked" || entry.message.priority === "urgent"),
	);
}

/**
 * Records the running service's feed into `file` (JSON lines) until the returned stop is called. The service is the
 * one `<data>/jocasta-service.json` names (`JOCASTA_DATA_DIR` picks a sandbox's).
 */
export async function recordFeed(file: string): Promise<{ stop(): void; closed: Promise<void> }> {
	const running = readServiceFile();
	if (!running) throw new Error("no running service: <data>/jocasta-service.json is missing");
	mkdirSync(dirname(file), { recursive: true });
	const options: Bun.WebSocketOptions = { headers: { authorization: `Bearer ${running.token}` } };
	// Bun takes headers here; the DOM lib's overload, which TypeScript picks, knows only protocols.
	const socket = new WebSocket(`ws://127.0.0.1:${running.port}${WS_PATH}`, options as unknown as string[]);
	const opened = Promise.withResolvers<void>();
	const closed = Promise.withResolvers<void>();
	socket.addEventListener("open", () => opened.resolve());
	socket.addEventListener("error", () => opened.reject(new Error("the service's socket refused the connection")));
	socket.addEventListener("close", () => closed.resolve());
	socket.addEventListener("message", (event) => {
		let message: unknown;
		try {
			message = JSON.parse(String(event.data));
		} catch {
			return;
		}
		appendFileSync(file, `${JSON.stringify({ at: Date.now(), message })}\n`);
	});
	await opened.promise;
	return { stop: () => socket.close(), closed: closed.promise };
}
