// How the `jocasta` herdr plugin's programs reach the running Jocasta service (0553): its port and token from
// `<data>/jocasta-service.json` (herdr starts them with the service's environment, so the same data dir) when its process is
// alive, the state feed over the WebSocket, his thread keys and his questions over HTTP with the token as a bearer.

import { connectionRefused, NOT_RUNNING, runningService } from "../shared/service-file.ts";
import {
	type ClientMessage,
	PLUGIN_ACTION_PATH,
	type PluginAction,
	POPUP_HOLD,
	POPUP_PATH,
	POPUP_READER_HEADER,
	type PopupAnswer,
	type PopupView,
	type ServerMessage,
	WS_PATH,
} from "./protocol.ts";

/** The running service's address and token, or undefined while none runs (no file, or its process is gone). */
export function serviceAt(): { base: string; token: string } | undefined {
	const at = runningService();
	return at && { base: `http://127.0.0.1:${at.port}`, token: at.token };
}

/** Why a call to the service failed: nothing listening is the one "not running" text (0581 UX-13). */
function failed(error: unknown): string {
	return connectionRefused(error) ? NOT_RUNNING : `Jocasta didn't answer: ${(error as Error).message}`;
}

/** Sends one of his thread keys to the service (0554); resolves why not when it wasn't taken, else undefined. */
export async function postAction(action: PluginAction, paneId: string | undefined): Promise<string | undefined> {
	const at = serviceAt();
	if (!at) return NOT_RUNNING;
	try {
		const response = await fetch(`${at.base}${PLUGIN_ACTION_PATH}`, {
			method: "POST",
			headers: { authorization: `Bearer ${at.token}`, "content-type": "application/json" },
			body: JSON.stringify({ action, ...(paneId ? { paneId } : {}) }),
			signal: AbortSignal.timeout(10_000),
		});
		return response.ok ? undefined : `Jocasta answered ${response.status}: ${await response.text()}`;
	} catch (error) {
		return failed(error);
	}
}

/** A question as its one reader got it (0578): its texts and the reader secret its hold and answer carry. */
export interface Question {
	code: string;
	view: PopupView;
	reader: string;
}

/** One question (`GET /api/popup/<code>`, its first and only read), or why not. */
export async function questionView(code: string): Promise<Question | string> {
	const at = serviceAt();
	if (!at) return NOT_RUNNING;
	try {
		const response = await fetch(`${at.base}${POPUP_PATH}${code}`, {
			headers: { authorization: `Bearer ${at.token}` },
			signal: AbortSignal.timeout(10_000),
		});
		if (!response.ok) return response.status === 403 ? "This question was already answered." : `${response.status}`;
		return {
			code,
			view: (await response.json()) as PopupView,
			reader: response.headers.get(POPUP_READER_HEADER) ?? "",
		};
	} catch (error) {
		return failed(error);
	}
}

/**
 * The service's reply to an answer (0578): taken; a wrong code with the tries left; the third wrong code, which took
 * the fallback; or why it failed.
 */
export type AnswerReply =
	| { kind: "taken" }
	| { kind: "wrong"; left?: number }
	| { kind: "exhausted" }
	| { kind: "failed"; text: string };

/** His answer (`POST /api/popup/<code>` with the reader secret). */
export async function answerQuestion(question: Question, answer: PopupAnswer): Promise<AnswerReply> {
	const at = serviceAt();
	if (!at) return { kind: "failed", text: NOT_RUNNING };
	try {
		const response = await fetch(`${at.base}${POPUP_PATH}${question.code}`, {
			method: "POST",
			headers: {
				authorization: `Bearer ${at.token}`,
				"content-type": "application/json",
				[POPUP_READER_HEADER]: question.reader,
			},
			body: JSON.stringify(answer),
			signal: AbortSignal.timeout(10_000),
		});
		if (response.ok) return { kind: "taken" };
		if (response.status === 410) return { kind: "exhausted" };
		if (response.status === 409) {
			const left = ((await response.json().catch(() => undefined)) as { left?: unknown } | undefined)?.left;
			return typeof left === "number" ? { kind: "wrong", left } : { kind: "wrong" };
		}
		return { kind: "failed", text: `Jocasta answered ${response.status}: ${await response.text()}` };
	} catch (error) {
		return { kind: "failed", text: failed(error) };
	}
}

/**
 * Holds the question's `GET /api/popup/<code>/hold` open while the popup shows it (0558); `onEnd` runs once when it
 * ends: the service ended the question (answered, withdrawn, Jocasta closing) or went away. The returned release ends
 * the hold, which takes the fallback when the question is still open (closing the popup any other way denies).
 */
export function holdQuestion(question: Question, onEnd: () => void): () => void {
	const controller = new AbortController();
	let ended = false;
	const end = () => {
		if (ended) return;
		ended = true;
		onEnd();
	};
	const at = serviceAt();
	if (!at) {
		end();
		return () => controller.abort();
	}
	void (async () => {
		try {
			const response = await fetch(`${at.base}${POPUP_PATH}${question.code}${POPUP_HOLD}`, {
				headers: { authorization: `Bearer ${at.token}`, [POPUP_READER_HEADER]: question.reader },
				signal: controller.signal,
			});
			if (!response.ok || !response.body) return;
			for await (const _ of response.body) {
				// Only its end matters.
			}
		} catch {
			// Ended either way.
		} finally {
			end();
		}
	})();
	return () => controller.abort();
}

/** The state feed `stateFeed` returns. */
export interface StateFeed {
	/** Sends a client message; false when the feed isn't connected (nothing was sent). */
	send(message: ClientMessage): boolean;
	/** Stops the feed: no reconnect, and no more calls to `onUp` or `onMessage`. */
	close(): void;
}

/**
 * The service's state feed (`/ws`): every message to `onMessage`; `onUp` with whether it is connected. Reconnects
 * every 2 s while the service is down or restarting (a new port or token in `jocasta-service.json`).
 */
export function stateFeed(onMessage: (message: ServerMessage) => void, onUp: (up: boolean) => void): StateFeed {
	let socket: WebSocket | undefined;
	let closed = false;
	let timer: Timer | undefined;
	const again = () => {
		if (!closed) timer = setTimeout(open, 2_000);
	};
	const open = () => {
		const at = serviceAt();
		if (!at) {
			onUp(false);
			again();
			return;
		}
		// Bun's constructor takes `{headers}`; the DOM typings in this project know only protocols.
		const ws = new WebSocket(`${at.base.replace(/^http/, "ws")}${WS_PATH}`, {
			headers: { authorization: `Bearer ${at.token}` },
		} as unknown as string[]);
		socket = ws;
		ws.onopen = () => {
			if (!closed && socket === ws) onUp(true);
		};
		ws.onmessage = (event) => {
			if (closed || socket !== ws) return;
			let message: ServerMessage;
			try {
				message = JSON.parse(String(event.data)) as ServerMessage;
			} catch {
				return; // Not one of the service's messages.
			}
			// Outside the parse: a failure drawing it is the pane's own error, not a message to skip (FE-5).
			onMessage(message);
		};
		ws.onclose = () => {
			if (closed || socket !== ws) return;
			socket = undefined;
			onUp(false);
			again();
		};
		ws.onerror = () => ws.close();
	};
	open();
	return {
		send(message) {
			if (socket?.readyState !== WebSocket.OPEN) return false;
			socket.send(JSON.stringify(message));
			return true;
		},
		close() {
			closed = true;
			clearTimeout(timer);
			socket?.close();
			socket = undefined;
		},
	};
}
