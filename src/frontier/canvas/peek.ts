/// <reference lib="dom" />
// whiteboard's code peeks on Jocasta's canvas page (0393, 0411): the authored ranges of one file as a read-only Monaco
// view numbered as in the file, and find over peeks that aren't mounted. A peek may cite both sides of its file
// (whiteboard groups a step's peeks by file and marks other-side ranges with their own `side`, CodePeek.tsx); each
// range is read on its own side, as the Desktop does (`side: range.side ?? spec.side`).
import type {
	ReviewDiffSide,
	ReviewFindQuery,
	ReviewInlineEditorHandle,
	ReviewInlineEditorRange,
	ReviewInlineEditorSpec,
	ReviewInlineFindSpec,
} from "./contract.ts";
import { loadMonaco, modelName, wholeLines } from "./editor.ts";
import type { monaco } from "./monaco.ts";
import { Emitter, type ReviewSource } from "./source.ts";

/** Monaco's default word separators: whole-word find, as the editor's own find widget does. */
const WORD_SEPARATORS = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";
/** A capped code peek's most height, in px. */
const CAPPED_HEIGHT = 420;

/** A shown line's place in its file; null for the `⋯` line between blocks. */
export type SliceLine = { side: ReviewDiffSide; line: number } | null;

/** A peek's model text and where each of its lines comes from. */
export interface Sliced {
	text: string;
	lines: SliceLine[];
}

/** The side a peek's range reads from. */
const sideOf = (range: ReviewInlineEditorRange, side: ReviewDiffSide): ReviewDiffSide => range.side ?? side;

/**
 * The ranges as one model: the default side's blocks first, then the other side's (the order whiteboard's
 * `mergedCodePeekRanges` emits), each side's ranges merged and in order, and one empty `⋯` line between blocks.
 * Ranges are clipped to their file; with no ranges the default side's whole file shows. `lines` keeps each shown
 * line's side and number, for the gutter and for finding the authored selections.
 */
export function slices(
	texts: Partial<Record<ReviewDiffSide, string>>,
	ranges: readonly ReviewInlineEditorRange[],
	side: ReviewDiffSide,
): Sliced {
	const order: ReviewDiffSide[] = side === "head" ? ["head", "base"] : ["base", "head"];
	const blocks: { side: ReviewDiffSide; all: string[]; start: number; end: number }[] = [];
	for (const at of order) {
		const all = (texts[at] ?? "").split("\n");
		const own = ranges.filter((r) => sideOf(r, side) === at);
		if (ranges.length === 0 && at === side) own.push({ startLine: 1, endLine: all.length });
		const sorted = own
			.map((r) => ({ start: Math.max(1, r.startLine), end: Math.min(all.length, Math.max(r.startLine, r.endLine)) }))
			.filter((r) => r.start <= r.end)
			.sort((a, b) => a.start - b.start);
		let last: (typeof blocks)[number] | undefined;
		for (const range of sorted) {
			if (last && range.start <= last.end + 1) last.end = Math.max(last.end, range.end);
			else {
				last = { side: at, all, ...range };
				blocks.push(last);
			}
		}
	}
	const out: string[] = [];
	const lines: SliceLine[] = [];
	blocks.forEach((block, i) => {
		if (i > 0) {
			out.push("");
			lines.push(null);
		}
		for (let n = block.start; n <= block.end; n++) {
			out.push(block.all[n - 1] ?? "");
			lines.push({ side: block.side, line: n });
		}
	});
	return { text: out.join("\n"), lines };
}

/** The sides a peek's ranges read from: the default side with no ranges. */
function sides(spec: ReviewInlineFindSpec): ReviewDiffSide[] {
	if (spec.ranges.length === 0) return [spec.side];
	return [...new Set(spec.ranges.map((r) => sideOf(r, spec.side)))];
}

/** A peek's model text: each side it cites read with the peek's pins (else the review's view), then sliced. */
async function peekSlices(source: ReviewSource, spec: ReviewInlineFindSpec): Promise<Sliced> {
	const at = spec.pins ? { pins: spec.pins } : {};
	const read = await Promise.all(
		sides(spec).map(async (side) => [side, await source.text(spec.path, side, at)] as const),
	);
	return slices(Object.fromEntries(read), spec.ranges, spec.side);
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

/** `inlineEditors.find`: the match count of a peek that isn't mounted, over the same model a mounted one shows. */
export async function findInPeek(
	source: ReviewSource,
	spec: ReviewInlineFindSpec,
	query: ReviewFindQuery,
): Promise<{ matchCount: number }> {
	const [sliced, api] = await Promise.all([peekSlices(source, spec), loadMonaco(source.worker)]);
	const model = api.monaco.editor.createModel(sliced.text);
	try {
		return { matchCount: findMatches(model, query).length };
	} finally {
		model.dispose();
	}
}

/** A code peek (`inlineEditors.create`): the authored ranges of one file, read-only, numbered as in the file. */
export function inlineEditor(source: ReviewSource, spec: ReviewInlineEditorSpec): ReviewInlineEditorHandle {
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
		const [sliced, api] = await Promise.all([peekSlices(source, spec), loadMonaco(source.worker)]);
		if (disposed) return;
		model = api.fileModel(sliced.text, spec.path, modelName("peek"));
		editor = api.monaco.editor.create(el, {
			...api.VIEW_OPTIONS,
			model,
			folding: false,
			lineNumbersMinChars: 4,
			lineNumbers: (n) => {
				const line = sliced.lines[n - 1];
				return line ? String(line.line) : "⋯";
			},
		});
		// The authored selections stand out when the shown ranges are wider, each found on its own side.
		const at = (range: ReviewInlineEditorRange, line: number) =>
			sliced.lines.findIndex((l) => l?.side === sideOf(range, spec.side) && l.line === line) + 1;
		const counted = (spec.countRanges ?? []).flatMap((range) => {
			const first = at(range, range.startLine);
			const last = at(range, range.endLine);
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
