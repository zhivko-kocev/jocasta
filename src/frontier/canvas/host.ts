/// <reference lib="dom" />
// Jocasta as whiteboard's canvas host (0393, 0411): the `ReviewCanvasBridge` the canvas mounts with, as whiteboard's
// Code-OSS desktop implements it (`apps/review-desktop/code-oss/src/vs/review/browser/parts/canvas/reviewCanvasPart.ts`,
// `services/reviewApiSourceService.ts`, `services/reviewDesktopConnectionService.ts`): requests go straight to
// whiteboard's loopback server with its token, code peeks and diffs are Monaco (0411) on Tokyo Night (0401), "open the
// code" shows a source panel over the canvas, and the server's `/control` stream (the Desktop's slot) lets an agent's
// `open` show its review here.
import type {
	ReviewApiSummary,
	ReviewCanvasBridge,
	ReviewCanvasContent,
	ReviewCanvasHandle,
	ReviewCanvasModule,
	ReviewDiffSide,
	ReviewInlineEditorRange,
	ReviewSourcePins,
	ReviewSurfaceEvent,
	ReviewVerbRequest,
	ReviewVerbResponse,
	WhiteboardConnection,
} from "./contract.ts";
import { attachControl } from "./control.ts";
import { diffView, layout } from "./diff.ts";
import { showMenu } from "./menu.ts";
import { SourcePanel } from "./panel.ts";
import { findInPeek, inlineEditor } from "./peek.ts";
import { disposable, Emitter, failure, ReviewSource } from "./source.ts";

/** How long a request to whiteboard's server may take before it counts as failed. */
const REQUEST_TIMEOUT_MS = 30_000;

/** Verbs a Code-OSS window answers that Jocasta's canvas page has no part for. */
const unsupported = (name: string): ReviewVerbResponse => ({ ok: false, error: `${name} isn't available in Jocasta` });

export interface HostEvents {
	/** The shown review's title, or null on Home. */
	onTitle(title: string | null): void;
	/**
	 * True on Home while the catalog holds no review but the scratchpad: the page shows Jocasta's empty state instead of
	 * whiteboard's Welcome (0419). False once anything is mounted.
	 */
	onEmpty(empty: boolean): void;
}

/**
 * The canvas page (0552): one mount of whiteboard's canvas that shows Home (the review list) or one review, and
 * the control stream that lets agents open reviews here.
 */
export class WhiteboardHost {
	private handle: ReviewCanvasHandle | undefined;
	private readonly surface = new Emitter<ReviewSurfaceEvent>();
	private readonly panel: SourcePanel;
	private readonly control = new AbortController();
	private reviewId: string | null = null;
	private homeTimer: number | undefined;
	/** Home's refresh while it is shown; undefined on a review. */
	private homeRender: (() => Promise<void>) | undefined;
	/** Home's refresh in flight: a poll while one is still out adds no second request (FE-14). */
	private homeLoad: Promise<void> | undefined;
	/** The review list Home last mounted, as JSON: an unchanged list isn't mounted again (PM-5). */
	private homeShown: string | undefined;
	/** Home's callbacks, the same each refresh, so an unchanged list stays unchanged for the canvas. */
	private readonly openFromHome = (id: string) => this.openReview(id);
	private readonly tutorialFromHome = () => void this.openTutorial();
	/** The poll stops while the window is hidden and Home refreshes once when it shows again. */
	private readonly onVisibility = () => {
		this.pollHome();
		if (!document.hidden) void this.refreshHome().catch(() => {});
	};

	constructor(
		private readonly module: ReviewCanvasModule,
		private readonly connection: WhiteboardConnection,
		private readonly container: HTMLElement,
		overlay: HTMLElement,
		private readonly events: HostEvents,
	) {
		// Monaco loads on first use (`loadMonaco`), with the worker each review's source carries.
		this.panel = new SourcePanel(overlay);
		void attachControl(connection, this.control.signal, (data) => this.answer(data));
		document.addEventListener("visibilitychange", this.onVisibility);
	}

	private mount(content: ReviewCanvasContent): void {
		this.homeShown = undefined;
		if (this.handle) this.handle.update(content);
		else
			this.handle = this.module.mountReviewCanvas(this.container, content, {
				confirmDelete: async (title) => window.confirm(`Delete “${title}”?`),
				showMenu,
			});
	}

	/** A request to whiteboard's server with its token; a hung server fails it after 30 s (FE-14). */
	private async api(path: string, init: RequestInit = {}): Promise<Response> {
		const headers = new Headers(init.headers);
		headers.set("x-review-token", this.connection.token);
		const timeout = AbortSignal.timeout(REQUEST_TIMEOUT_MS);
		const signal = init.signal ? AbortSignal.any([init.signal, timeout]) : timeout;
		const response = await fetch(`${this.connection.url}${path}`, { ...init, headers, signal });
		if (!response.ok) throw await failure(response);
		return response;
	}

	/** Home's refresh, unless one is still in flight. */
	private refreshHome(): Promise<void> {
		const render = this.homeRender;
		if (!render) return Promise.resolve();
		this.homeLoad ??= render().finally(() => {
			this.homeLoad = undefined;
		});
		return this.homeLoad;
	}

	/** Home's 5 s refresh, only while Home and the window show. */
	private pollHome(): void {
		clearInterval(this.homeTimer);
		this.homeTimer = undefined;
		if (!this.homeRender || document.hidden) return;
		this.homeTimer = window.setInterval(() => void this.refreshHome().catch(() => {}), 5_000);
	}

	/**
	 * Home: every review in whiteboard's catalog, refreshed while shown. With none but the scratchpad, Home would be
	 * whiteboard's Welcome, whose steps can't complete inside Jocasta; the page shows its own empty state instead (0419).
	 */
	async showHome(): Promise<void> {
		this.reviewId = null;
		this.panel.close();
		this.events.onTitle(null);
		this.homeRender = async () => {
			const body: unknown = await (await this.api("/reviews-api")).json();
			if (this.reviewId !== null) return;
			const reviews = Array.isArray(body)
				? body.filter(
						(r): r is ReviewApiSummary =>
							r !== null && typeof r === "object" && "reviewId" in r && typeof r.reviewId === "string",
					)
				: [];
			// whiteboard's own test (`homeSessions`): the scratchpad doesn't count as a review.
			if (!reviews.some((r) => r.kind !== "scratchpad")) {
				this.homeShown = undefined;
				this.events.onEmpty(true);
				return;
			}
			this.events.onEmpty(false);
			const shown = JSON.stringify(reviews);
			if (shown === this.homeShown) return;
			this.mount({ kind: "home", reviews, openReview: this.openFromHome, openTutorial: this.tutorialFromHome });
			this.homeShown = shown;
		};
		this.homeShown = undefined;
		this.pollHome();
		try {
			await this.refreshHome();
		} catch (error) {
			// A review opened while Home's request was out stays open.
			if (this.reviewId === null) {
				this.events.onEmpty(false);
				this.mount({ kind: "error", message: (error as Error).message });
			}
		}
	}

	/** whiteboard's tutorial review (`POST /tutorial/open`), opened on the canvas. */
	async openTutorial(): Promise<void> {
		const body: unknown = await (await this.api("/tutorial/open", { method: "POST" })).json();
		if (body && typeof body === "object" && "reviewUuid" in body && typeof body.reviewUuid === "string") {
			this.openReview(body.reviewUuid);
		}
	}

	/** One review on the canvas (the boss's `show`, an agent's `open`, or a click on Home). */
	openReview(reviewId: string): void {
		if (reviewId === this.reviewId) return;
		this.homeRender = undefined;
		this.pollHome();
		this.reviewId = reviewId;
		this.panel.close();
		this.events.onEmpty(false);
		this.events.onTitle("");
		const source = new ReviewSource(this.connection, reviewId);
		const showCode = async (
			target: { file: string; side: ReviewDiffSide; pins?: ReviewSourcePins | undefined },
			range: ReviewInlineEditorRange,
		) => this.panel.showCode(source, target, range);
		const bridge: ReviewCanvasBridge = {
			config: {
				serverUrl: this.connection.url,
				reviewId,
				token: this.connection.token,
				wasmUrl: new URL(this.connection.canvas.wasm, location.href).href,
				appVersion: this.connection.version,
				theme: "dark",
				host: "desktop",
			},
			inlineEditors: {
				create: (spec) => inlineEditor(source, spec),
				find: (spec, query) => findInPeek(source, spec, query),
			},
			diffView: {
				create: (spec) => diffView(source, spec),
				files: (scope) => source.files(scope?.commit),
			},
			request: (url, init) => fetch(url, init),
			post: async (request) => {
				switch (request.name) {
					case "reveal":
						await showCode(
							{ file: request.args.path, side: request.args.side ?? "head", pins: request.args.pins },
							request.args,
						);
						return { ok: true };
					case "openDiff":
						await this.panel.showDiff(source, request.args.path);
						return { ok: true };
					case "showReviewView":
						this.surface.fire({ event: "showReviewView", view: request.args.view });
						return { ok: true };
					default:
						return this.dispatch(request);
				}
			},
			subscribe: (listener) => this.surface.add(listener),
			currentTheme: () => "dark",
			onDidChangeTheme: () => disposable(() => {}),
			currentDiffLayout: () => layout.get(),
			setDiffLayout: async (value) => layout.set(value),
			onDidChangeDiffLayout: (listener) => layout.changed.add(listener),
			ready: () => {
				this.container.dataset.reviewCanvasReady = "true";
			},
			reportDiagnostic: (diagnostic) =>
				(diagnostic.level === "error" ? console.error : console.warn)(
					`[whiteboard ${diagnostic.source}] ${diagnostic.message}`,
				),
		};
		delete this.container.dataset.reviewCanvasReady;
		this.mount({
			kind: "api",
			reviewId,
			bridge,
			setTitle: (title) => this.events.onTitle(title),
			setSourceView: (_selection, view) => {
				source.view = view;
			},
			openSource: (target, range) => showCode(target, range),
		});
	}

	/** The verbs any window answers: from the canvas, or from an agent through the control stream. */
	private async dispatch(request: ReviewVerbRequest): Promise<ReviewVerbResponse> {
		switch (request.name) {
			case "authoringCapabilities":
				// Software maps route: the page's policy lets libavoid's glue build its functions (0493).
				return { ok: true, result: { softwareMapEnabled: true } };
			case "openReview":
				this.openReview(request.args.reviewUuid);
				return { ok: true };
			case "openApiReview":
				this.openReview(request.args.reviewId);
				return { ok: true };
			case "focusWindow":
				window.focus();
				return { ok: true };
			default:
				return unsupported(request.name);
		}
	}

	/** One control frame: the verb it carries, answered on `POST /control/result` with the frame's id. */
	private async answer(data: string): Promise<void> {
		let frame: unknown;
		try {
			frame = JSON.parse(data);
		} catch {
			return;
		}
		if (!frame || typeof frame !== "object" || !("id" in frame) || !("request" in frame)) return;
		if (typeof frame.id !== "string" || !frame.request || typeof frame.request !== "object") return;
		// A verb whiteboard's server validated against its own schema before relaying it.
		const request = frame.request as ReviewVerbRequest;
		let response: ReviewVerbResponse;
		try {
			response = await this.dispatch(request);
		} catch (error) {
			response = { ok: false, error: (error as Error).message };
		}
		await this.api("/control/result", {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify({ id: frame.id, response }),
			signal: this.control.signal,
		}).catch(() => {});
	}

	dispose(): void {
		this.control.abort();
		this.homeRender = undefined;
		this.pollHome();
		document.removeEventListener("visibilitychange", this.onVisibility);
		this.panel.close();
		this.handle?.dispose();
		this.handle = undefined;
	}
}

/** The canvas build's files didn't load: Try again reloads the page when the script failed (FE-8). */
export class CanvasFilesError extends Error {
	constructor(
		message: string,
		/** The browser keeps a failed module import for the page's life, so only a reload fetches it again. */
		readonly reload: boolean,
	) {
		super(message);
	}
}

/**
 * Loads whiteboard's canvas build from the service: its stylesheets, then its module (Zod's CSP-safe mode first, as
 * the Desktop sets it, since the page allows no eval).
 */
export async function loadCanvas(connection: WhiteboardConnection): Promise<ReviewCanvasModule> {
	const zodConfig: unknown = Reflect.get(globalThis, "__zod_globalConfig");
	Reflect.set(globalThis, "__zod_globalConfig", {
		...(zodConfig && typeof zodConfig === "object" ? zodConfig : {}),
		jitless: true,
	});
	for (const href of connection.canvas.styles) {
		if (document.querySelector(`link[href="${CSS.escape(href)}"]`)) continue;
		const { promise, resolve, reject } = Promise.withResolvers<void>();
		const link = document.createElement("link");
		link.rel = "stylesheet";
		link.href = href;
		link.onload = () => resolve();
		link.onerror = () => {
			// A failed link left in the page would be skipped above on Try again, and the canvas mounted unstyled.
			link.remove();
			reject(new CanvasFilesError(`couldn't load ${href}`, false));
		};
		document.head.append(link);
		await promise;
	}
	// Runtime-selected: the canvas is whiteboard's own build, named by its vite manifest and served by the service;
	// the page's bundle can't import it statically.
	let loaded: unknown;
	try {
		loaded = await import(/* @vite-ignore */ connection.canvas.script);
	} catch (error) {
		throw new CanvasFilesError(`couldn't load ${connection.canvas.script}: ${(error as Error).message}`, true);
	}
	if (!loaded || typeof loaded !== "object" || !("mountReviewCanvas" in loaded)) {
		throw new Error("whiteboard's canvas bundle has no mount function");
	}
	const { mountReviewCanvas } = loaded;
	if (typeof mountReviewCanvas !== "function") throw new Error("whiteboard's canvas bundle has no mount function");
	// The canvas build's `mountReviewCanvas(container, content, ui)` (desktop-entry.tsx).
	return { mountReviewCanvas: mountReviewCanvas as ReviewCanvasModule["mountReviewCanvas"] };
}
