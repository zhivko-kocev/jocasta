// How the `jocasta` herdr plugin's programs reach the running Jocasta service (0553): its port and token from
// `<data>/frontier.json` (herdr starts them with the service's environment, so the same data dir), the state feed over
// the WebSocket, his thread keys and his questions over HTTP with the token as a bearer.

import {
	PLUGIN_ACTION_PATH,
	type PluginAction,
	POPUP_HOLD,
	POPUP_PATH,
	type PopupAnswer,
	type PopupView,
	type ServerMessage,
	WS_PATH,
} from "../protocol.ts";
import { readFrontierFile } from "../server/frontier-file.ts";

/** The running service's address and token, or undefined while none runs. */
export function serviceAt(): { base: string; token: string } | undefined {
	const file = readFrontierFile();
	return file ? { base: `http://127.0.0.1:${file.port}`, token: file.token } : undefined;
}

const NO_SERVICE = "Jocasta's service isn't running.";

/** Sends one of his thread keys to the service (0554); resolves why not when it wasn't taken, else undefined. */
export async function postAction(action: PluginAction, paneId: string | undefined): Promise<string | undefined> {
	const at = serviceAt();
	if (!at) return NO_SERVICE;
	try {
		const response = await fetch(`${at.base}${PLUGIN_ACTION_PATH}`, {
			method: "POST",
			headers: { authorization: `Bearer ${at.token}`, "content-type": "application/json" },
			body: JSON.stringify({ action, ...(paneId ? { paneId } : {}) }),
			signal: AbortSignal.timeout(10_000),
		});
		return response.ok ? undefined : `Jocasta answered ${response.status}: ${await response.text()}`;
	} catch (error) {
		return `Jocasta didn't answer: ${(error as Error).message}`;
	}
}

/** One question's texts (`GET /api/popup/<code>`), or why not. */
export async function questionView(code: string): Promise<PopupView | string> {
	const at = serviceAt();
	if (!at) return NO_SERVICE;
	try {
		const response = await fetch(`${at.base}${POPUP_PATH}${code}`, {
			headers: { authorization: `Bearer ${at.token}` },
			signal: AbortSignal.timeout(10_000),
		});
		if (!response.ok) return response.status === 403 ? "This question was already answered." : `${response.status}`;
		return (await response.json()) as PopupView;
	} catch (error) {
		return `Jocasta didn't answer: ${(error as Error).message}`;
	}
}

/** His answer (`POST /api/popup/<code>`): `taken`, `wrong` (a coded choice's letters), or why not. */
export async function answerQuestion(code: string, answer: PopupAnswer): Promise<"taken" | "wrong" | string> {
	const at = serviceAt();
	if (!at) return NO_SERVICE;
	try {
		const response = await fetch(`${at.base}${POPUP_PATH}${code}`, {
			method: "POST",
			headers: { authorization: `Bearer ${at.token}`, "content-type": "application/json" },
			body: JSON.stringify(answer),
			signal: AbortSignal.timeout(10_000),
		});
		if (response.ok) return "taken";
		if (response.status === 409) return "wrong";
		return `Jocasta answered ${response.status}: ${await response.text()}`;
	} catch (error) {
		return `Jocasta didn't answer: ${(error as Error).message}`;
	}
}

/**
 * Holds the question's `GET /api/popup/<code>/hold` open while the popup shows it (0558); `onEnd` runs once when it
 * ends: the service ended the question (answered, withdrawn, Jocasta closing) or went away.
 */
export function holdQuestion(code: string, onEnd: () => void): () => void {
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
			const response = await fetch(`${at.base}${POPUP_PATH}${code}${POPUP_HOLD}`, {
				headers: { authorization: `Bearer ${at.token}` },
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

/**
 * The service's state feed (`/ws`): every message to `onMessage`; `onUp` with whether it is connected. Reconnects
 * every 2 s while the service is down or restarting (a new port or token in `frontier.json`). Returns `send` for client
 * messages and `close`.
 */
export function stateFeed(
	onMessage: (message: ServerMessage) => void,
	onUp: (up: boolean) => void,
): { send(message: object): void; close(): void } {
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
		ws.onopen = () => onUp(true);
		ws.onmessage = (event) => {
			try {
				onMessage(JSON.parse(String(event.data)) as ServerMessage);
			} catch {
				// Not one of the service's messages.
			}
		};
		ws.onclose = () => {
			if (socket !== ws) return;
			socket = undefined;
			onUp(false);
			again();
		};
		ws.onerror = () => ws.close();
	};
	open();
	return {
		send(message) {
			if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
		},
		close() {
			closed = true;
			clearTimeout(timer);
			socket?.close();
		},
	};
}
