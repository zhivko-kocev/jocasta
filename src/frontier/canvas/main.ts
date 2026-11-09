/// <reference lib="dom" />
// The canvas page (0393, 0552): whiteboard's canvas in his browser, at `/?review=<id>` from the boss's `show` or at `/`
// for whiteboard's Home (the review list). While a review is open a slim bar above it goes back to the review list and
// names the review; before that there is no bar (0432). The service hands over whiteboard's server and the canvas build
// (`GET /whiteboard/connection`). Until the first review exists the stage shows Jocasta's empty state, not
// whiteboard's Welcome (0419); when the canvas can't start it says why in plain words and what to do (0432).
import { loadCanvas, WhiteboardHost } from "./bridge.ts";
import type { WhiteboardConnection } from "./contract.ts";
import { canvasTrouble } from "./trouble.ts";

/** `GET /whiteboard/connection`: whiteboard's server and canvas files, or why there are none. */
async function connect(): Promise<WhiteboardConnection> {
	const response = await fetch("/whiteboard/connection");
	const body: unknown = await response.json().catch(() => null);
	if (!response.ok || !body || typeof body !== "object" || !("url" in body)) {
		const error = body && typeof body === "object" && "error" in body ? String(body.error) : `${response.status}`;
		throw new Error(error);
	}
	// The service's own reply (server.ts `whiteboardConnection`).
	return body as WhiteboardConnection;
}

/** An element with its class and, when given, its text. */
function element<K extends keyof HTMLElementTagNameMap>(
	tag: K,
	className: string,
	text?: string,
): HTMLElementTagNameMap[K] {
	const made = document.createElement(tag);
	if (className) made.className = className;
	if (text !== undefined) made.textContent = text;
	return made;
}

/** One of the page's small buttons. */
function button(text: string, onClick: () => void): HTMLButtonElement {
	const made = element("button", "btn small", text);
	made.type = "button";
	made.addEventListener("click", onClick);
	return made;
}

/**
 * The page: the bar while a review is open, the trouble line when the canvas can't start, and the stage with
 * whiteboard's mount, Jocasta's empty state and the source panel's overlay.
 */
class CanvasPage {
	readonly root = element("div", "wb");
	private readonly bar = element("div", "wb-bar");
	private readonly title = element("span", "wb-title");
	private readonly stage = element("div", "wb-stage");
	private readonly mount = element("div", "wb-mount");
	private readonly overlay = element("div", "wb-overlay");
	private empty: HTMLElement | undefined;
	private trouble: HTMLElement | undefined;
	private host: WhiteboardHost | null = null;
	/** Each start's number: a start a later one replaced comes to nothing. */
	private attempt = 0;

	/** `review`: the review the page opens with, else Home. */
	constructor(private readonly review: string | null) {
		this.bar.append(
			button("Reviews", () => void this.host?.showHome()),
			this.title,
		);
		this.stage.append(this.mount, this.overlay);
		this.root.append(this.stage);
	}

	/** Connects to whiteboard and mounts its canvas, showing the review or Home; again after Try again. */
	start(): void {
		const attempt = ++this.attempt;
		this.host?.dispose();
		this.host = null;
		this.trouble?.remove();
		this.trouble = undefined;
		this.showTitle(null);
		this.showEmpty(false);
		this.open(attempt).catch((caught: Error) => {
			if (attempt === this.attempt) this.showTrouble(caught.message);
		});
	}

	private async open(attempt: number): Promise<void> {
		const connection = await connect();
		const module = await loadCanvas(connection);
		if (attempt !== this.attempt) return;
		const host = new WhiteboardHost(module, connection, this.mount, this.overlay, {
			onTitle: (title) => this.showTitle(title),
			onEmpty: (empty) => this.showEmpty(empty),
		});
		this.host = host;
		if (this.review) host.openReview(this.review);
		else await host.showHome();
	}

	/** The bar while a review is open (its title, "" while it loads), none on Home. */
	private showTitle(title: string | null): void {
		document.title = title ? `${title} · Jocasta` : "Jocasta";
		if (title === null) {
			this.bar.remove();
			return;
		}
		this.title.textContent = title || "Review";
		if (!this.bar.isConnected) this.root.prepend(this.bar);
	}

	/** Jocasta's empty state instead of whiteboard's Welcome while the catalog holds no review (0419). */
	private showEmpty(empty: boolean): void {
		this.mount.hidden = empty;
		if (!empty) {
			this.empty?.remove();
			this.empty = undefined;
			return;
		}
		if (this.empty) return;
		const shown = element("div", "wb-empty");
		const notice = element("p", "notice error");
		notice.setAttribute("role", "alert");
		shown.append(
			element("h2", "", "Nothing on the canvas yet."),
			element("p", "", "Ask Jocasta or a thread to draw a diagram or a review; it appears here."),
			button("Take the whiteboard tour", () => {
				notice.remove();
				this.host?.openTutorial().catch((caught: Error) => {
					notice.textContent = `The tour didn't open: ${caught.message}`;
					shown.append(notice);
				});
			}),
		);
		this.empty = shown;
		this.overlay.before(shown);
	}

	/** Why the canvas couldn't start, Try again when it can help, and the raw error under Details (0432). */
	private showTrouble(error: string): void {
		const trouble = canvasTrouble(error);
		const shown = element("div", "wb-unavailable");
		shown.setAttribute("role", "alert");
		shown.append(
			element("h2", "", "The canvas couldn't start."),
			element("p", "", `${trouble.reason} ${trouble.remedy}`),
		);
		if (trouble.transient) shown.append(button("Try again", () => this.start()));
		const details = element("details", "");
		details.append(element("summary", "", "Details"), element("code", "", error));
		shown.append(details);
		this.trouble = shown;
		this.stage.before(shown);
	}
}

const page = new CanvasPage(new URLSearchParams(location.search).get("review"));
(document.getElementById("root") as HTMLElement).append(page.root);
page.start();
