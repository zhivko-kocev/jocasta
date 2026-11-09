/// <reference lib="dom" />
// What every Monaco part of the canvas host shares (0411): Monaco loaded once on first use, whole-line ranges, the
// line height the views use, and unique model names. Light on purpose: this module never loads Monaco's chunk itself.
import type * as MonacoApi from "./monaco.ts";
import type { monaco } from "./monaco.ts";

/** Every view's line height in px (`VIEW_OPTIONS.lineHeight`); a not-yet-mounted diff's placeholder is sized with it. */
export const LINE_HEIGHT = 19;

let monacoLoad: Promise<typeof MonacoApi> | undefined;

/**
 * Monaco on first use (PM-2): Home and diagrams never need it, so its 5 MB chunk loads with the first code peek, diff
 * or source panel, set up once with the service's diff worker.
 */
export const loadMonaco = (worker: string): Promise<typeof MonacoApi> =>
	(monacoLoad ??= import("./monaco.ts").then((m) => {
		m.setupMonaco(worker);
		return m;
	}));

/** Whole lines `start` to `end`, as Monaco's `IRange`. */
export const wholeLines = (start: number, end: number): monaco.IRange => ({
	startLineNumber: start,
	startColumn: 1,
	endLineNumber: end,
	endColumn: 1,
});

let modelTag = 0;

/** A new model name's tag (`peek-3`): models of the same path in two views stay apart. */
export const modelName = (kind: string): string => `${kind}-${modelTag++}`;
