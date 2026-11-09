/// <reference lib="dom" />
// The source panel over Jocasta's canvas (0393, 0411): what "open the code" and a step's file link show, a whole file
// at a range or one file's diff, read-only Monaco on Tokyo Night.
import type { ReviewDiffFileWire, ReviewDiffSide, ReviewDisposable, ReviewSourcePins } from "./contract.ts";
import { fileDiff } from "./diff.ts";
import { loadMonaco, modelName, wholeLines } from "./editor.ts";
import type * as MonacoApi from "./monaco.ts";
import { disposable, type ReviewSource } from "./source.ts";

/** What Tab reaches inside the source panel. */
const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

/** The side a range is on, in words: "before the change" (base) or "after the change" (head). */
const sideWords = (side: string): string =>
	side === "base" || side === "old" ? "before the change" : "after the change";

/**
 * The source panel over the canvas: a whole file at a range, or one file's diff. It takes focus when it opens, Escape
 * (anywhere) or Close shuts it, and focus goes back to what opened it. A file that can't be read says so in the panel.
 */
export class SourcePanel {
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
		const model = api.fileModel(text, target.file, modelName("source"));
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

	/** `openDiff`: one changed file's diff, built at once. */
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
		part.mount();
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
