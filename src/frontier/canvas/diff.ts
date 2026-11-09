/// <reference lib="dom" />
// whiteboard's Diff tab and a document's embedded diff on Jocasta's canvas page (0393, 0411): each changed file's
// Monaco diff, base against head, unchanged regions folded, built only when it nears the screen (FE-6).
import type {
	ReviewDiffFileWire,
	ReviewDiffLayout,
	ReviewDiffViewHandle,
	ReviewDiffViewSpec,
	ReviewDisposable,
} from "./contract.ts";
import { LINE_HEIGHT, loadMonaco, modelName } from "./editor.ts";
import type { monaco } from "./monaco.ts";
import { Emitter, type ReviewSource } from "./source.ts";

/** The diff layout outlives the window, as the Desktop's `diffEditor.renderSideBySide` setting does. */
const LAYOUT_KEY = "jocasta.whiteboard.diffLayout";
/** The most lines a not-yet-built diff's placeholder stands for. */
const PLACEHOLDER_LINES = 40;
/** How far from the screen a file's diff starts building, so it is usually ready when scrolled to. */
const NEAR = "600px 0px";

/** The diff layout, kept in the page's storage. */
export const layout = {
	changed: new Emitter<ReviewDiffLayout>(),
	get(): ReviewDiffLayout {
		return localStorage.getItem(LAYOUT_KEY) === "unified" ? "unified" : "split";
	},
	set(value: ReviewDiffLayout): void {
		localStorage.setItem(LAYOUT_KEY, value);
		layout.changed.fire(value);
	},
};

/** One file's diff in a list: its section, and `mount` to build its editor (once). */
export interface FileDiff extends ReviewDisposable {
	readonly section: HTMLElement;
	mount(): void;
}

/**
 * One file's diff, base against head, unchanged regions folded. Its header shows at once, its body as a placeholder
 * the height of its changed lines (capped) until `mount` reads both texts and builds the editor.
 */
export function fileDiff(
	host: HTMLElement,
	source: ReviewSource,
	file: ReviewDiffFileWire,
	commit: string | undefined,
	onHeight: () => void,
): FileDiff {
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
		return { section, mount() {}, dispose: () => section.remove() };
	}
	body.style.height = `${Math.min(file.additions + file.deletions, PLACEHOLDER_LINES) * LINE_HEIGHT}px`;
	let editor: monaco.editor.IStandaloneDiffEditor | undefined;
	const models: monaco.editor.ITextModel[] = [];
	let mounted = false;
	let disposed = false;
	const layoutSub = layout.changed.add((value) => editor?.updateOptions({ renderSideBySide: value === "split" }));
	const build = async () => {
		const at = commit ? { commit } : {};
		const [before, after, api] = await Promise.all([
			file.status === "added" ? "" : source.text(file.previousPath ?? file.path, "base", at),
			file.status === "deleted" ? "" : source.text(file.path, "head", at),
			loadMonaco(source.worker),
		]);
		if (disposed) return;
		const tag = modelName("diff");
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
	};
	return {
		section,
		mount() {
			if (mounted || disposed) return;
			mounted = true;
			build().catch((error: Error) => {
				body.style.height = "";
				body.textContent = error.message;
				body.classList.add("wb-error");
				onHeight();
			});
		},
		dispose() {
			disposed = true;
			layoutSub.dispose();
			editor?.dispose();
			for (const model of models) model.dispose();
			section.remove();
		},
	};
}

/**
 * The Diff tab and a document's embedded diff (`diffView.create`): every changed file, or the lens's files, each
 * built as it nears the screen; `revealFile` builds its file at once.
 */
export function diffView(source: ReviewSource, spec: ReviewDiffViewSpec): ReviewDiffViewHandle {
	const errors = new Emitter<string>();
	const el = document.createElement("div");
	el.className = spec.document ? "wb-diff wb-diff-doc" : "wb-diff";
	// The canvas focuses the Diff tab through `focus()`; a div takes focus only with a tabIndex.
	el.tabIndex = -1;
	spec.container.append(el);
	const parts = new Map<string, FileDiff>();
	let disposed = false;
	const reportHeight = () => spec.document?.onDidChangeHeight(el.scrollHeight);
	// The Diff tab scrolls itself; a document's diff scrolls with the document, inside the viewport.
	const near = new IntersectionObserver(
		(entries) => {
			for (const entry of entries) {
				if (!entry.isIntersecting || !(entry.target instanceof HTMLElement)) continue;
				near.unobserve(entry.target);
				parts.get(entry.target.dataset.path ?? "")?.mount();
			}
		},
		{ root: spec.document ? null : el, rootMargin: NEAR },
	);
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
		for (const file of files) {
			const part = fileDiff(el, source, file, spec.scope?.commit, reportHeight);
			parts.set(file.path, part);
			near.observe(part.section);
		}
		reportHeight();
	})().catch((error: Error) => {
		el.textContent = error.message;
		el.classList.add("wb-error");
		errors.fire(error.message);
	});
	return {
		focus: () => el.focus(),
		revealFile(path) {
			const part = parts.get(path);
			if (!part) return;
			near.unobserve(part.section);
			part.mount();
			part.section.scrollIntoView({ block: "start" });
		},
		onDidError: (listener) => errors.add(listener),
		dispose() {
			disposed = true;
			near.disconnect();
			for (const part of parts.values()) part.dispose();
			el.remove();
		},
	};
}
