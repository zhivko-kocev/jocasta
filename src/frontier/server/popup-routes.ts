import { log } from "../../shared/log.ts";
import {
	PLUGIN_ACTION_PATH,
	PLUGIN_ACTIONS,
	type PluginAction,
	POPUP_HOLD,
	POPUP_PATH,
	POPUP_READER_HEADER,
	type PopupView,
	type ServerMessage,
} from "../protocol.ts";
import type { CloseOutcome } from "./close.ts";
import { cleanText } from "./herdr.ts";
import { type ActionDeps, type AskPopups, runAction } from "./plugin.ts";
import type { Popup, Popups } from "./popups.ts";
import { bodyObject, hasBearer, type PageRoute, type Route, refusedPost } from "./routes.ts";
import type { JocastaService } from "./service.ts";

/** One refusal notification per text this often (0578); `notifications.jsonl` still logs each. */
const REFUSAL_TOAST_MS = 30_000;

/** Asks him a question: `popups` opens it, `show` shows it (0519, 0552). */
export type Ask = (view: PopupView, signal?: AbortSignal, options?: { fromKey?: boolean }) => Popup;

/** Opens each question in `popups` and hands it to `show`; it ends unanswered on `signal`. */
export function asker(popups: Popups, show: ((popup: Popup) => void) | undefined): Ask {
	return (view, signal, options = {}) => {
		const popup = popups.open(view, options);
		if (signal?.aborted) popup.close();
		signal?.addEventListener("abort", () => popup.close(), { once: true });
		if (popup.open) {
			log("info", "question", `asking him: ${popup.view.title}`);
			try {
				show?.(popup);
			} catch (error) {
				log("warn", "question", `couldn't show it: ${(error as Error).message}`);
			}
		}
		return popup;
	};
}

/** What his questions' routes need. */
export interface PopupParts {
	token: string;
	popups: Popups;
	askPopups: AskPopups | undefined;
	service: Pick<JocastaService, "notify">;
}

/**
 * His questions' routes: the `ask` popup's held request (0558), before the page fallback; and `/api/popup/<code>`, its
 * texts and its answer (0519, 0578), among the page's bearer routes.
 */
export function popupRoutes({ token, popups, askPopups, service }: PopupParts): { hold: Route; question: PageRoute } {
	/** A second program read his question (0578): it ended as Deny or Cancel, and he is told. */
	const secondReader = (popup: Popup) => {
		log("warn", "question", `a second program read the question, which counted as ${popup.view.fallback}`);
		void service.notify({
			type: "toast",
			reason: "failed",
			level: "warning",
			text: `Jocasta — Another program read your question "${popup.view.message}"; it counted as Deny.`,
		});
	};

	/**
	 * A question's texts and its answer, each with the service's token (checked before) and the question's one-time
	 * code: an unknown or used code is refused. The first read hands out the reader secret, which the answer must
	 * carry; a second read ends the question (410). Answers: 204 taken; 409 `{left}` a wrong code; 410 the third wrong
	 * code, which took the fallback.
	 */
	const popupResponse = async (request: Request, url: URL): Promise<Response> => {
		const code = url.pathname.slice(POPUP_PATH.length);
		const refused = () => new Response("Forbidden", { status: 403 });
		if (code.includes("/")) return new Response("Not found", { status: 404 });
		if (request.method === "GET") {
			const read = popups.read(code);
			if (!read) return refused();
			if ("second" in read) {
				secondReader(read.second);
				return new Response("read already", { status: 410 });
			}
			return Response.json(read.view, {
				headers: { "cache-control": "no-store", [POPUP_READER_HEADER]: read.reader },
			});
		}
		if (request.method !== "POST") return new Response("Method not allowed", { status: 405 });
		if (!popups.isReader(code, request.headers.get(POPUP_READER_HEADER))) return refused();
		// An answer without a body is no choice.
		const taken = popups.answer(code, await bodyObject(request));
		if (taken === "unknown") return refused();
		if (taken === "invalid") return new Response("not one of its choices", { status: 400 });
		// A coded choice with letters that aren't its code (0558, 0568): it stays open, until the third.
		if (taken === "wrong") return Response.json({ left: popups.left(code) }, { status: 409 });
		if (taken === "exhausted") return new Response("wrong code three times", { status: 410 });
		return new Response(null, { status: 204 });
	};

	return {
		// The `ask` popup's held request (0558): open for as long as it shows its question; the bearer and the reader
		// secret of its first read (0578). A hold for an open question without that secret ends it as a second read does.
		hold: {
			matches: (path) => path.startsWith(POPUP_PATH) && path.endsWith(POPUP_HOLD),
			handle: (request, url, server) => {
				const code = url.pathname.slice(POPUP_PATH.length, -POPUP_HOLD.length);
				if (!hasBearer(request, token) || !askPopups) return new Response("Forbidden", { status: 403 });
				if (!popups.isReader(code, request.headers.get(POPUP_READER_HEADER))) {
					const open = popups.get(code);
					if (!open) return new Response("Forbidden", { status: 403 });
					open.close("second-reader");
					secondReader(open);
					return new Response("read already", { status: 410 });
				}
				server.timeout(request, 0);
				return askPopups.hold(code, request.signal);
			},
		},
		question: {
			matches: (path) => path.startsWith(POPUP_PATH),
			handle: (request, url) =>
				hasBearer(request, token) ? popupResponse(request, url) : new Response("Forbidden", { status: 403 }),
		},
	};
}

/** What his thread keys' route needs. */
export interface ActionParts {
	token: string;
	service: ActionDeps["service"] & Pick<JocastaService, "notify">;
	ask: Ask;
	askPopups: AskPopups | undefined;
	broadcast: (message: ServerMessage) => void;
	/** Jocasta's close from his quit key, asking first. */
	quit: () => Promise<CloseOutcome>;
	/** How long Finish and New thread wait for his answer. */
	waitMs: number;
}

/**
 * `POST /api/plugin/action` (0544, 0554): one of his thread keys, `{action, paneId?}` with the service's token,
 * answered at once; what it did follows as an `action` message, and a refusal or half-done outcome as a notification
 * too: he may be on another workspace. A notification of the same text goes out once per 30 s;
 * `notifications.jsonl` logs each.
 */
export function actionRoute(parts: ActionParts): Route {
	const { token, service, askPopups, broadcast } = parts;
	/** When each refusal text last went out as a notification (0578). */
	const refusalsShown = new Map<string, number>();
	const reportAction = (said: string, isError: boolean) => {
		const text = cleanText(said);
		log(isError ? "warn" : "info", "keys", text);
		broadcast({ type: "action", text, isError });
		if (!isError) return;
		const now = Date.now();
		for (const [shown, at] of refusalsShown) if (now - at >= REFUSAL_TOAST_MS) refusalsShown.delete(shown);
		const show = !refusalsShown.has(text);
		if (show) refusalsShown.set(text, now);
		// Logged in notifications.jsonl as the service's toasts are (0552).
		void service.notify({ type: "toast", reason: "failed", level: "warning", text }, { show });
	};

	/** His keys' open questions by (action, thread) (0578). */
	const keyQuestions = new Map<string, Popup>();
	const runKey = async (action: PluginAction, paneId: string | undefined): Promise<void> => {
		try {
			const outcome = await runAction(action, paneId, {
				service,
				ask: (view, signal) => parts.ask(view, signal, { fromKey: true }),
				front: (popup) => askPopups?.front(popup),
				quit: parts.quit,
				progress: (text) => reportAction(text, false),
				asking: keyQuestions,
				waitMs: parts.waitMs,
			});
			if (outcome) reportAction(outcome.text, outcome.isError);
		} catch (error) {
			reportAction(error instanceof Error ? error.message : String(error), true);
		}
	};

	return {
		matches: (path) => path === PLUGIN_ACTION_PATH,
		handle: async (request) => {
			const refused = refusedPost(request, token, false);
			if (refused) return refused;
			// No body: no action.
			const body: { action?: unknown; paneId?: unknown } = await bodyObject(request);
			const action = PLUGIN_ACTIONS.find((a) => a === body.action);
			if (!action) return new Response(`action is one of ${PLUGIN_ACTIONS.join(", ")}`, { status: 400 });
			const paneId = typeof body.paneId === "string" && body.paneId ? body.paneId : undefined;
			void runKey(action, paneId);
			return new Response(null, { status: 202 });
		},
	};
}
