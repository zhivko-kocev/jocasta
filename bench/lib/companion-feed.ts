import { appendFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { TOKEN_COOKIE, WS_PATH } from "../../src/frontier/protocol.ts";
import { readFrontierFile } from "../../src/frontier/server/frontier-file.ts";
import { jsonLines } from "./timing.ts";

/**
 * The service's feed, recorded (A5): the service's log holds warnings only, so toasts are taken from its state socket
 * (`/ws`, the token as a cookie), which broadcasts every snapshot, toast and HUD message; each message is appended with
 * its arrival time.
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

/** The toasts in a feed; `urgentOnly` keeps those the app also shows as a Windows notification (`service.ts` `send`). */
export function toastsOf(entries: readonly FeedEntry[], urgentOnly = false): Toast[] {
	return entries.filter(
		(entry): entry is Toast =>
			entry.message.type === "toast" &&
			(!urgentOnly || entry.message.reason === "blocked" || entry.message.priority === "urgent"),
	);
}

/**
 * Records the running companion's feed into `file` (JSON lines) until the returned stop is called. The companion is the
 * one `<data>/frontier.json` names (`JOCASTA_DATA_DIR` picks a sandbox's).
 */
export async function recordFeed(file: string): Promise<{ stop(): void; closed: Promise<void> }> {
	const frontier = readFrontierFile();
	if (!frontier) throw new Error("no running companion: <data>/frontier.json is missing");
	mkdirSync(dirname(file), { recursive: true });
	const options: Bun.WebSocketOptions = { headers: { cookie: `${TOKEN_COOKIE}=${frontier.token}` } };
	// Bun takes headers here; the DOM lib's overload, which TypeScript picks, knows only protocols.
	const socket = new WebSocket(`ws://127.0.0.1:${frontier.port}${WS_PATH}`, options as unknown as string[]);
	const opened = Promise.withResolvers<void>();
	const closed = Promise.withResolvers<void>();
	socket.addEventListener("open", () => opened.resolve());
	socket.addEventListener("error", () => opened.reject(new Error("the companion's socket refused the connection")));
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
