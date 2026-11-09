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
	ReviewDiffFileWire,
	ReviewDiffLayout,
	ReviewDiffSide,
	ReviewDiffViewSpec,
	ReviewDisposable,
	ReviewFindQuery,
	ReviewInlineEditorHandle,
	ReviewInlineEditorRange,
	ReviewInlineEditorSpec,
	ReviewMenuRequest,
	ReviewSourcePins,
	ReviewSourceView,
	ReviewSurfaceEvent,
	ReviewVerbRequest,
	ReviewVerbResponse,
	WhiteboardConnection,
} from "./contract.ts";
import type * as MonacoApi from "./monaco.ts";
import type { monaco } from "./monaco.ts";

/** Monaco's default word separators: whole-word find, as the editor's own find widget does. */
const WORD_SEPARATORS = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";
/** A capped code peek's most height, in px. */
const CAPPED_HEIGHT = 420;
/** The diff layout outlives the window, as the Desktop's `diffEditor.renderSideBySide` setting does. */
const LAYOUT_KEY = "jocasta.whiteboard.diffLayout";

const disposable = (dispose: () => void): ReviewDisposable => ({ dispose });

let monacoLoad: Promise<typeof MonacoApi> | undefined;

/**
 * Monaco on first use (PM-2): Home and diagrams never need it, so its 5 MB chunk loads with the first code peek, diff
 * or source panel, set up once with the service's diff worker.
 */
const loadMonaco = (worker: string): Promise<typeof MonacoApi> =>
	(monacoLoad ??= import("./monaco.ts").then((m) => {
		m.setupMonaco(worker);
		return m;
	}));

/** Whole lines `start` to `end`, as Monaco's `IRange`. */
const wholeLines = (start: number, end: number): monaco.IRange => ({
	startLineNumber: start,
	startColumn: 1,
	endLineNumber: end,
	endColumn: 1,
});

/** A listener set whose `add` returns its own removal. */
class Emitter<T> {
	private readonly listeners = new Set<(value: T) => void>();
	add(listener: (value: T) => void): ReviewDisposable {
		this.listeners.add(listener);
		return disposable(() => this.listeners.delete(listener));
	}
	fire(value: T): void {
		for (const listener of this.listeners) listener(value);
	}
}

/** What the server says went wrong, else the status. */
async function failure(response: Response): Promise<Error> {
	const body: unknown = await response.json().catch(() => null);
	const error = body && typeof body === "object" && "error" in body ? body.error : undefined;
	return new Error(typeof error === "string" && error ? error : `whiteboard answered ${response.status}`);
}

/** Where a peek or diff reads its source: the review's current view, or a reference's own pins. */
interface SourceAt {
	view: ReviewSourceView | undefined;
	pins?: ReviewSourcePins | undefined;
	commit?: string | undefined;
}

/** `reviewSourceQuery` (contracts.ts): the view's version and either its pins or its commit. */
function sourceQuery(at: SourceAt, extra: Record<string, string> = {}): URLSearchParams {
	const query = new URLSearchParams();
	if (at.view) query.set("version", String(at.view.version));
	const pins = at.pins ?? at.view?.pins;
	if (pins) {
		query.set("repositoryId", pins.repositoryId);
		query.set("head", pins.head);
		if (pins.base) query.set("base", pins.base);
	} else {
		const commit = at.commit ?? at.view?.commit;
		if (commit) query.set("commit", commit);
	}
	for (const [key, value] of Object.entries(extra)) query.set(key, value);
	return query;
}

/** One review's source reads from whiteboard's server (`GET /reviews-api/<id>/file|diff`). */
class ReviewSource {
	view: ReviewSourceView | undefined;
	constructor(
		private readonly connection: WhiteboardConnection,
		readonly reviewId: string,
	) {}

	/** Monaco's diff worker from the service. */
	get worker(): string {
		return this.connection.worker;
	}

	private async get(route: string, query: URLSearchParams): Promise<unknown> {
		const response = await fetch(
			`${this.connection.url}/reviews-api/${encodeURIComponent(this.reviewId)}${route}?${query}`,
			{ headers: { "x-review-token": this.connection.token }, signal: AbortSignal.timeout(30_000) },
		);
		if (!response.ok) throw await failure(response);
		return response.json();
	}

	/** A file's text on one side; a binary reads as a notice, as the Desktop shows it. */
	async text(path: string, side: ReviewDiffSide, at: Omit<SourceAt, "view"> = {}): Promise<string> {
		const body = await this.get(
			"/file",
			sourceQuery({ view: this.view, ...at }, { side, file: path, binary: "describe" }),
		);
		if (body && typeof body === "object") {
			if ("binary" in body && body.binary === true) return "Binary file cannot be displayed as text.";
			if ("text" in body && typeof body.text === "string") return body.text;
		}
		throw new Error(`whiteboard sent no text for ${path}`);
	}

	/** The changed files of the review's comparison, or of one commit. */
	async files(commit?: string): Promise<ReviewDiffFileWire[]> {
		const body = await this.get("/diff", sourceQuery({ view: this.view, commit }));
		if (!Array.isArray(body)) throw new Error("whiteboard sent no file list");
		return body.filter(
			(file): file is ReviewDiffFileWire =>
				file !== null && typeof file === "object" && "path" in file && typeof file.path === "string",
		);
	}
}

/** The diff layout, kept in the page's storage. */
const layout = {
	changed: new Emitter<ReviewDiffLayout>(),
	get(): ReviewDiffLayout {
		return localStorage.getItem(LAYOUT_KEY) === "unified" ? "unified" : "split";
	},
	set(value: ReviewDiffLayout): void {
		localStorage.setItem(LAYOUT_KEY, value);
		layout.changed.fire(value);
	},
};

/** The ranges, merged and in order, as one model: the lines between ranges become one `⋯` line. */
function slices(text: string, ranges: readonly ReviewInlineEditorRange[]): { text: string; lines: number[] } {
	const all = text.split("\n");
	const sorted = [...ranges]
		.map((r) => ({ start: Math.max(1, r.startLine), end: Math.min(all.length, Math.max(r.startLine, r.endLine)) }))
		.sort((a, b) => a.start - b.start);
	const merged: { start: number; end: number }[] = [];
	for (const range of sorted) {
		const last = merged.at(-1);
		if (last && range.start <= last.end + 1) last.end = Math.max(last.end, range.end);
		else merged.push({ ...range });
	}
	if (merged.length === 0) merged.push({ start: 1, end: all.length });
	const out: string[] = [];
	const lines: number[] = [];
	merged.forEach(({ start, end }, i) => {
		if (i > 0) {
			out.push("");
			lines.push(0);
		}
		for (let n = start; n <= end; n++) {
			out.push(all[n - 1] ?? "");
			lines.push(n);
		}
	});
	return { text: out.join("\n"), lines };
}

function findMatches(model: monaco.editor.ITextModel, query: ReviewFindQuery): monaco.editor.FindMatch[] {
	if (!query.text) return [];
	try {
		return model.findMatches(
			query.text,
			false,
			query.isRegex,
			query.matchCase,
			query.wholeWord ? WORD_SEPARATORS : null,
			false,
		);
	} catch {
		// An unfinished regular expression finds nothing yet.
		return [];
	}
}

let modelTag = 0;

/** A code peek (`inlineEditors.create`): the authored ranges of one file, read-only, numbered as in the file. */
function inlineEditor(source: ReviewSource, spec: ReviewInlineEditorSpec): ReviewInlineEditorHandle {
	const heights = new Emitter<number>();
	const errors = new Emitter<string>();
	const el = document.createElement("div");
	el.className = "wb-peek-code";
	spec.container.append(el);
	let height = 0;
	let editor: monaco.editor.IStandaloneCodeEditor | undefined;
	let model: monaco.editor.ITextModel | undefined;
	let matches: monaco.editor.FindMatch[] = [];
	let disposed = false;
	let found: monaco.editor.IEditorDecorationsCollection | undefined;

	const measure = () => {
		if (!editor) return;
		const content = editor.getContentHeight();
		const next = spec.heightMode === "capped" ? Math.min(content, CAPPED_HEIGHT) : content;
		if (next === height) return;
		height = next;
		el.style.height = `${next}px`;
		editor.layout();
		heights.fire(next);
	};

	const loaded = (async () => {
		const [text, api] = await Promise.all([
			source.text(spec.path, spec.side, spec.pins ? { pins: spec.pins } : {}),
			loadMonaco(source.worker),
		]);
		if (disposed) return;
		const sliced = slices(text, spec.ranges);
		model = api.fileModel(sliced.text, spec.path, `peek-${modelTag++}`);
		editor = api.monaco.editor.create(el, {
			...api.VIEW_OPTIONS,
			model,
			folding: false,
			lineNumbersMinChars: 4,
			lineNumbers: (n) => {
				const line = sliced.lines[n - 1];
				return line ? String(line) : "⋯";
			},
		});
		// The authored selections stand out when the shown ranges are wider.
		const counted = (spec.countRanges ?? []).flatMap((range) => {
			const first = sliced.lines.indexOf(range.startLine) + 1;
			const last = sliced.lines.indexOf(range.endLine) + 1;
			return first > 0 && last >= first
				? [{ range: wholeLines(first, last), options: { isWholeLine: true, className: "wb-counted" } }]
				: [];
		});
		if (counted.length > 0 && spec.countRanges?.length !== spec.ranges.length)
			editor.createDecorationsCollection(counted);
		editor.onDidContentSizeChange(measure);
		editor.onDidFocusEditorText(() => spec.onDidFocus?.());
		measure();
	})().catch((error: Error) => {
		el.textContent = error.message;
		el.classList.add("wb-error");
		errors.fire(error.message);
	});

	return {
		get height() {
			return height;
		},
		setActive: (active) => el.classList.toggle("active", active),
		setCollapsed: (collapsed) => {
			el.hidden = collapsed;
		},
		onDidChangeHeight: (listener) => heights.add(listener),
		onDidError: (listener) => errors.add(listener),
		async setFindQuery(query) {
			await loaded;
			if (!editor || !model) return { matchCount: 0 };
			matches = findMatches(model, query);
			found?.clear();
			found = editor.createDecorationsCollection(
				matches.map((m) => ({ range: m.range, options: { className: "findMatch", inlineClassName: "wb-find" } })),
			);
			return { matchCount: matches.length };
		},
		revealFindMatch(index) {
			const match = matches[index];
			if (!editor || !match) return;
			editor.setSelection(match.range);
			editor.revealRangeInCenterIfOutsideViewport(match.range);
		},
		clearActiveFindMatch: () => editor?.setSelection(wholeLines(1, 1)),
		clearFind() {
			matches = [];
			found?.clear();
		},
		dispose() {
			disposed = true;
			editor?.dispose();
			model?.dispose();
			el.remove();
		},
	};
}

/** One file's diff, base against head, unchanged regions folded. */
function fileDiff(
	host: HTMLElement,
	source: ReviewSource,
	file: ReviewDiffFileWire,
	commit: string | undefined,
	onHeight: () => void,
): ReviewDisposable {
	const section = document.createElement("section");
	section.className = "wb-diff-file";
	section.dataset.path = file.path;
	const head = document.createElement("header");
	const name = document.createElement("span");
	name.className = "path";
	name.textContent =
		file.previousPath && file.previousPath !== file.path ? `${file.previousPath} → ${file.path}` : file.path;
	const counts = document.createElement("span");
	counts.className = "counts";
	const added = document.createElement("b");
	added.className = "add";
	added.textContent = `+${file.additions}`;
	const deleted = document.createElement("b");
	deleted.className = "del";
	deleted.textContent = `−${file.deletions}`;
	counts.append(added, " ", deleted);
	head.append(name, counts);
	const body = document.createElement("div");
	body.className = "wb-diff-body";
	section.append(head, body);
	host.append(section);
	if (file.binary) {
		body.textContent = "Binary file.";
		return disposable(() => section.remove());
	}
	let editor: monaco.editor.IStandaloneDiffEditor | undefined;
	const models: monaco.editor.ITextModel[] = [];
	let disposed = false;
	const layoutSub = layout.changed.add((value) => editor?.updateOptions({ renderSideBySide: value === "split" }));
	void (async () => {
		const at = commit ? { commit } : {};
		const [before, after, api] = await Promise.all([
			file.status === "added" ? "" : source.text(file.previousPath ?? file.path, "base", at),
			file.status === "deleted" ? "" : source.text(file.path, "head", at),
			loadMonaco(source.worker),
		]);
		if (disposed) return;
		const tag = `diff-${modelTag++}`;
		const original = api.fileModel(before, file.previousPath ?? file.path, `${tag}-base`);
		const modified = api.fileModel(after, file.path, `${tag}-head`);
		models.push(original, modified);
		editor = api.monaco.editor.createDiffEditor(body, {
			...api.VIEW_OPTIONS,
			renderSideBySide: layout.get() === "split",
			useInlineViewWhenSpaceIsLimited: true,
			originalEditable: false,
			renderOverviewRuler: false,
			renderIndicators: true,
			hideUnchangedRegions: { enabled: true, contextLineCount: 3, minimumLineCount: 3, revealLineCount: 20 },
		});
		editor.setModel({ original, modified });
		const fit = () => {
			if (!editor) return;
			const h = Math.max(editor.getOriginalEditor().getContentHeight(), editor.getModifiedEditor().getContentHeight());
			body.style.height = `${h}px`;
			onHeight();
		};
		editor.getOriginalEditor().onDidContentSizeChange(fit);
		editor.getModifiedEditor().onDidContentSizeChange(fit);
		editor.onDidUpdateDiff(fit);
		fit();
	})().catch((error: Error) => {
		body.textContent = error.message;
		body.classList.add("wb-error");
	});
	return disposable(() => {
		disposed = true;
		layoutSub.dispose();
		editor?.dispose();
		for (const model of models) model.dispose();
		section.remove();
	});
}

/** The Diff tab and a document's embedded diff (`diffView.create`): every changed file, or the lens's files. */
function diffView(source: ReviewSource, spec: ReviewDiffViewSpec) {
	const errors = new Emitter<string>();
	const el = document.createElement("div");
	el.className = spec.document ? "wb-diff wb-diff-doc" : "wb-diff";
	spec.container.append(el);
	const parts: ReviewDisposable[] = [];
	let disposed = false;
	const reportHeight = () => spec.document?.onDidChangeHeight(el.scrollHeight);
	void (async () => {
		let files = await source.files(spec.scope?.commit);
		const lens = spec.lens;
		if (lens && !lens.wholeFiles) {
			files = files.filter((file) =>
				lens.ranges.some((r) => r.file === (r.side === "base" ? (file.previousPath ?? file.path) : file.path)),
			);
		}
		files = files.filter((file) => file.status !== "unchanged");
		if (disposed) return;
		if (files.length === 0) el.textContent = "No changed files.";
		for (const file of files) parts.push(fileDiff(el, source, file, spec.scope?.commit, reportHeight));
		reportHeight();
	})().catch((error: Error) => {
		el.textContent = error.message;
		el.classList.add("wb-error");
		errors.fire(error.message);
	});
	return {
		focus: () => el.focus(),
		revealFile: (path: string) =>
			el.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)?.scrollIntoView({ block: "start" }),
		onDidError: (listener: (message: string) => void) => errors.add(listener),
		dispose() {
			disposed = true;
			for (const part of parts) part.dispose();
			el.remove();
		},
	};
}

/** Where a key moves in a menu of `n` enabled items, from item `at`. */
const MENU_MOVES: Record<string, (at: number, n: number) => number> = {
	ArrowDown: (at, n) => (at + 1) % n,
	ArrowUp: (at, n) => (at - 1 + n) % n,
	Home: () => 0,
	End: (_at, n) => n - 1,
};

/** What Tab reaches inside the source panel. */
const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

/**
 * A menu the canvas asks for (`ReviewCanvasUi.showMenu`), under its anchor and inside the window. It takes the focus
 * on its first enabled item; the arrows move (wrapping), Home and End jump, Tab closes it, and Escape or a choice
 * closes it and gives the focus back to the anchor (UX-7).
 */
function showMenu(request: ReviewMenuRequest): ReviewDisposable {
	const menu = document.createElement("div");
	menu.className = "wb-menu";
	menu.setAttribute("role", "menu");
	const box = request.anchor.getBoundingClientRect();
	menu.style.top = `${box.bottom + 4}px`;
	let open = true;
	const close = () => {
		if (!open) return;
		open = false;
		menu.remove();
		document.removeEventListener("pointerdown", outside, true);
		document.removeEventListener("keydown", onKey, true);
		request.onHide();
	};
	const outside = (event: Event) => {
		if (!(event.target instanceof Node) || !menu.contains(event.target)) close();
	};
	const buttons: HTMLButtonElement[] = [];
	const enabled = () => buttons.filter((b) => !b.disabled);
	const onKey = (event: KeyboardEvent) => {
		if (event.key === "Escape") {
			event.stopPropagation();
			event.preventDefault();
			close();
			request.anchor.focus();
			return;
		}
		if (event.key === "Tab") {
			// The focus leaves from the anchor, as if the menu had never opened.
			close();
			request.anchor.focus();
			return;
		}
		const items = enabled();
		const move = MENU_MOVES[event.key];
		if (!move || items.length === 0 || !(event.target instanceof Node) || !menu.contains(event.target)) return;
		event.preventDefault();
		items[move(items.indexOf(event.target as HTMLButtonElement), items.length)]?.focus();
	};
	for (const item of request.items) {
		const button = document.createElement("button");
		button.type = "button";
		button.setAttribute("role", item.checked === undefined ? "menuitem" : "menuitemcheckbox");
		if (item.checked !== undefined) button.setAttribute("aria-checked", String(item.checked));
		button.disabled = item.enabled === false;
		button.textContent = item.label;
		button.addEventListener("click", () => {
			close();
			request.anchor.focus();
			void request.onSelect(item.id);
		});
		buttons.push(button);
		menu.append(button);
	}
	document.body.append(menu);
	menu.style.left = `${Math.max(8, Math.min(box.left, innerWidth - menu.offsetWidth - 8))}px`;
	document.addEventListener("pointerdown", outside, true);
	document.addEventListener("keydown", onKey, true);
	enabled()[0]?.focus();
	return disposable(close);
}

/** The side a range is on, in words: "before the change" (base) or "after the change" (head). */
const sideWords = (side: string): string =>
	side === "base" || side === "old" ? "before the change" : "after the change";

/**
 * The source panel over the canvas: a whole file at a range, or one file's diff. It takes focus when it opens, Escape
 * (anywhere) or Close shuts it, and focus goes back to what opened it. A file that can't be read says so in the panel.
 */
class SourcePanel {
	private current: ReviewDisposable | undefined;
	/** What had focus before the panel opened; it gets focus back on close. */
	private opener: Element | null = null;
	constructor(private readonly host: HTMLElement) {}

	private open(title: string, detail: string): HTMLElement {
		if (!this.current) this.opener = document.activeElement;
		this.current?.dispose();
		const panel = document.createElement("div");
		panel.className = "wb-source";
		panel.setAttribute("role", "dialog");
		panel.setAttribute("aria-label", title);
		panel.tabIndex = -1;
		const head = document.createElement("header");
		const name = document.createElement("span");
		name.className = "path";
		name.textContent = title;
		const meta = document.createElement("span");
		meta.className = "meta";
		meta.textContent = detail;
		const close = document.createElement("button");
		close.type = "button";
		close.className = "btn small";
		close.textContent = "Close";
		close.addEventListener("click", () => this.close());
		head.append(name, meta, close);
		const body = document.createElement("div");
		body.className = "wb-source-body";
		panel.append(head, body);
		// Escape closes it wherever focus is (Monaco's or the canvas's), and goes no further while it is open.
		const onEscape = (event: KeyboardEvent) => {
			if (event.key !== "Escape") return;
			event.stopPropagation();
			this.close();
		};
		document.addEventListener("keydown", onEscape, true);
		// Tab and Shift+Tab cycle among the panel's own controls while it is open: the canvas under it is hidden (UX-18).
		panel.addEventListener("keydown", (event) => {
			if (event.key !== "Tab" || event.defaultPrevented) return;
			const focusable = [...panel.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
				(el) => !el.matches(":disabled") && el.getClientRects().length > 0,
			);
			const first = focusable[0];
			const last = focusable.at(-1);
			if (!first || !last) return;
			const active = document.activeElement;
			if (event.shiftKey && (active === first || active === panel)) {
				event.preventDefault();
				last.focus();
			} else if (!event.shiftKey && (active === last || active === panel)) {
				event.preventDefault();
				first.focus();
			}
		});
		this.host.append(panel);
		panel.focus();
		this.current = disposable(() => {
			document.removeEventListener("keydown", onEscape, true);
			panel.remove();
		});
		return body;
	}

	/** A plain line in the panel when its file or diff can't be shown. */
	private failed(body: HTMLElement, what: string, error: unknown): void {
		const line = document.createElement("p");
		line.className = "wb-error";
		line.textContent = `${what} couldn't be read: ${error instanceof Error ? error.message : String(error)}`;
		body.replaceChildren(line);
	}

	/** `reveal` and `openSource`: the file, read-only, scrolled to the range and lit. */
	async showCode(
		source: ReviewSource,
		target: { file: string; side: ReviewDiffSide; pins?: ReviewSourcePins | undefined },
		range: { startLine: number; endLine: number },
	): Promise<void> {
		const lines =
			range.endLine > range.startLine ? `lines ${range.startLine}–${range.endLine}` : `line ${range.startLine}`;
		const body = this.open(target.file, `${lines} · ${sideWords(target.side)}`);
		let text: string;
		let api: typeof MonacoApi;
		try {
			[text, api] = await Promise.all([
				source.text(target.file, target.side, target.pins ? { pins: target.pins } : {}),
				loadMonaco(source.worker),
			]);
		} catch (error) {
			this.failed(body, target.file, error);
			return;
		}
		// Closed or replaced while the file was read.
		if (!body.isConnected) return;
		const model = api.fileModel(text, target.file, `source-${modelTag++}`);
		const editor = api.monaco.editor.create(body, { ...api.VIEW_OPTIONS, model });
		editor.createDecorationsCollection([
			{ range: wholeLines(range.startLine, range.endLine), options: { isWholeLine: true, className: "wb-lit" } },
		]);
		editor.revealLinesInCenter(range.startLine, range.endLine);
		editor.setSelection(wholeLines(range.startLine, range.startLine));
		editor.focus();
		const previous = this.current;
		this.current = disposable(() => {
			editor.dispose();
			model.dispose();
			previous?.dispose();
		});
		document.body.dataset.whiteboardSource = target.file;
	}

	/** `openDiff`: one changed file's diff. */
	async showDiff(source: ReviewSource, path: string): Promise<void> {
		const body = this.open(path, "diff");
		let file: ReviewDiffFileWire | undefined;
		try {
			file = (await source.files()).find((f) => f.path === path);
		} catch (error) {
			this.failed(body, path, error);
			return;
		}
		if (!body.isConnected) return;
		if (!file) {
			this.failed(body, path, new Error("it isn't changed in this review"));
			return;
		}
		const part = fileDiff(body, source, file, undefined, () => {});
		const previous = this.current;
		this.current = disposable(() => {
			part.dispose();
			previous?.dispose();
		});
	}

	close(): void {
		this.current?.dispose();
		this.current = undefined;
		delete document.body.dataset.whiteboardSource;
		const opener = this.opener;
		this.opener = null;
		if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
	}
}

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
	/** The review list Home last mounted, as JSON: an unchanged list isn't mounted again (PM-5). */
	private homeShown: string | undefined;
	/** Home's callbacks, the same each refresh, so an unchanged list stays unchanged for the canvas. */
	private readonly openFromHome = (id: string) => this.openReview(id);
	private readonly tutorialFromHome = () => void this.openTutorial();
	/** The poll stops while the window is hidden and Home refreshes once when it shows again. */
	private readonly onVisibility = () => {
		this.pollHome();
		if (this.homeRender && !document.hidden) void this.homeRender().catch(() => {});
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
		void this.attachControl();
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

	private async api(path: string, init: RequestInit = {}): Promise<Response> {
		const headers = new Headers(init.headers);
		headers.set("x-review-token", this.connection.token);
		const response = await fetch(`${this.connection.url}${path}`, { ...init, headers });
		if (!response.ok) throw await failure(response);
		return response;
	}

	/** Home's 5 s refresh, only while Home and the window show. */
	private pollHome(): void {
		clearInterval(this.homeTimer);
		this.homeTimer = undefined;
		const render = this.homeRender;
		if (!render || document.hidden) return;
		this.homeTimer = window.setInterval(() => void render().catch(() => {}), 5_000);
	}

	/**
	 * Home: every review in whiteboard's catalog, refreshed while shown. With none but the scratchpad, Home would be
	 * whiteboard's Welcome, whose steps can't complete inside Jocasta; the page shows its own empty state instead (0419).
	 */
	async showHome(): Promise<void> {
		this.reviewId = null;
		this.panel.close();
		this.events.onTitle(null);
		const render = async () => {
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
		this.homeRender = render;
		this.homeShown = undefined;
		this.pollHome();
		try {
			await render();
		} catch (error) {
			this.events.onEmpty(false);
			this.mount({ kind: "error", message: (error as Error).message });
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
				async find(spec, query) {
					const [text, api] = await Promise.all([
						source.text(spec.path, spec.side, spec.pins ? { pins: spec.pins } : {}),
						loadMonaco(source.worker),
					]);
					const model = api.monaco.editor.createModel(slices(text, spec.ranges).text);
					try {
						return { matchCount: findMatches(model, query).length };
					} finally {
						model.dispose();
					}
				},
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

	/**
	 * The Desktop's slot on whiteboard's server (`GET /control`, server-sent events of `desktop-verb` frames, answered on
	 * `POST /control/result`): while it's attached, an agent's `open` shows its review on this canvas. Reattaches
	 * after a drop until the host goes.
	 */
	private async attachControl(): Promise<void> {
		const signal = this.control.signal;
		while (!signal.aborted) {
			try {
				const url = new URL("/control", this.connection.url);
				url.searchParams.set("token", this.connection.token);
				const response = await fetch(url, { signal });
				if (!response.ok || !response.body) throw await failure(response);
				const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
				let buffered = "";
				for (;;) {
					const { value, done } = await reader.read();
					if (done) break;
					buffered += value;
					for (let end = buffered.search(/\r?\n\r?\n/); end >= 0; end = buffered.search(/\r?\n\r?\n/)) {
						const block = buffered.slice(0, end);
						buffered = buffered.slice(end).replace(/^\r?\n\r?\n/, "");
						const data = block
							.split(/\r?\n/)
							.filter((line) => line.startsWith("data:"))
							.map((line) => line.slice(5).replace(/^ /, ""))
							.join("\n");
						if (data) await this.answer(data);
					}
				}
			} catch {
				// Dropped or refused (another Desktop holds the slot): try again shortly.
			}
			if (!signal.aborted) {
				const { promise, resolve } = Promise.withResolvers<void>();
				setTimeout(resolve, 3_000);
				await promise;
			}
		}
	}

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
		link.onerror = () => reject(new Error(`couldn't load ${href}`));
		document.head.append(link);
		await promise;
	}
	// Runtime-selected: the canvas is whiteboard's own build, named by its vite manifest and served by the service;
	// the page's bundle can't import it statically.
	const loaded: unknown = await import(/* @vite-ignore */ connection.canvas.script);
	if (!loaded || typeof loaded !== "object" || !("mountReviewCanvas" in loaded)) {
		throw new Error("whiteboard's canvas bundle has no mount function");
	}
	const { mountReviewCanvas } = loaded;
	if (typeof mountReviewCanvas !== "function") throw new Error("whiteboard's canvas bundle has no mount function");
	// The canvas build's `mountReviewCanvas(container, content, ui)` (desktop-entry.tsx).
	return { mountReviewCanvas: mountReviewCanvas as ReviewCanvasModule["mountReviewCanvas"] };
}
