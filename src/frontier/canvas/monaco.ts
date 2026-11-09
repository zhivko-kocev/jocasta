// Monaco 0.57.0 as whiteboard's host editor (0411): read-only code ranges and diffs for the canvas's code peeks, its
// Diff tab and the source panel, on Tokyo Night (0401). Only what a viewer needs is imported (as in the editors demo,
// docs/research/editors-demo/): the core API, the diff editor, find, folding and the languages' tokenizers, which load
// per language on first use.
import * as monaco from "monaco-editor/editor/editor.api.js";
import "monaco-editor/editor/browser/widget/diffEditor/diffEditor.contribution.js";
import "monaco-editor/features/find/register.js";
import "monaco-editor/editor/contrib/folding/browser/folding.js";
import "monaco-editor/languages/definitions/register.all.js";
import "../../../node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon.css";
import "../../../node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon-modifiers.css";
import { LINE_HEIGHT } from "./editor.ts";
import { CODE, LINE_OPACITY, MONO_FONT, TOKYO_NIGHT, withAlpha, withOpacity } from "./palette.ts";

export { monaco };

/** Tokyo Night as the page draws it, from palette.css (0401, 0474). */
const TOKYO = {
	bg: TOKYO_NIGHT.bg0,
	bg1: TOKYO_NIGHT.bg1,
	bg2: TOKYO_NIGHT.bg2,
	fg: TOKYO_NIGHT.fg,
	fg3: TOKYO_NIGHT.mark3,
	line: withOpacity(TOKYO_NIGHT.comment, LINE_OPACITY.line1),
	lineStrong: withOpacity(TOKYO_NIGHT.comment, LINE_OPACITY.line2),
	blue: TOKYO_NIGHT.blue,
	green: TOKYO_NIGHT.green,
	red: TOKYO_NIGHT.red,
} as const;
/** Fully transparent: not a colour of the palette. */
const CLEAR = "#00000000";

const hex = (colour: string) => colour.slice(1);

let ready = false;

/** Monaco's worker (diffs) from the service, and the Tokyo Night theme; once per page. */
export function setupMonaco(workerUrl: string): void {
	if (ready) return;
	ready = true;
	Object.assign(self, { MonacoEnvironment: { getWorker: () => new Worker(workerUrl, { type: "module" }) } });
	monaco.editor.defineTheme("tokyo-night", {
		base: "vs-dark",
		inherit: false,
		rules: [
			{ token: "", foreground: hex(CODE.ink) },
			{ token: "comment", foreground: hex(CODE.comment), fontStyle: "italic" },
			{ token: "keyword", foreground: hex(CODE.keyword) },
			{ token: "string", foreground: hex(CODE.string) },
			{ token: "string.escape", foreground: hex(CODE.number) },
			{ token: "regexp", foreground: hex(CODE.string) },
			{ token: "number", foreground: hex(CODE.number) },
			{ token: "type", foreground: hex(CODE.type) },
			{ token: "type.identifier", foreground: hex(CODE.type) },
			{ token: "identifier", foreground: hex(CODE.ink) },
			{ token: "delimiter", foreground: hex(CODE.quiet) },
			{ token: "operator", foreground: hex(CODE.quiet) },
			{ token: "tag", foreground: hex(CODE.keyword) },
			{ token: "attribute.name", foreground: hex(CODE.type) },
			{ token: "attribute.value", foreground: hex(CODE.string) },
			{ token: "invalid", foreground: hex(CODE.error) },
		],
		colors: {
			"editor.background": TOKYO.bg,
			"editor.foreground": TOKYO.fg,
			"editorLineNumber.foreground": TOKYO.fg3,
			"editorLineNumber.activeForeground": CODE.quiet,
			"editor.lineHighlightBackground": CLEAR,
			"editor.lineHighlightBorder": CLEAR,
			"editor.selectionBackground": withAlpha(TOKYO.blue, 0x33),
			"editor.findMatchBackground": withAlpha(TOKYO.blue, 0x66),
			"editor.findMatchHighlightBackground": withAlpha(TOKYO.blue, 0x33),
			"editor.rangeHighlightBackground": withAlpha(TOKYO.blue, 0x1a),
			"editorCursor.foreground": TOKYO.blue,
			"editorIndentGuide.background1": TOKYO.line,
			"editorIndentGuide.activeBackground1": TOKYO.lineStrong,
			"editorGutter.background": TOKYO.bg,
			"editorWidget.background": TOKYO.bg2,
			"editorWidget.border": TOKYO.lineStrong,
			"scrollbarSlider.background": withAlpha(TOKYO_NIGHT.comment, 0x33),
			"scrollbarSlider.hoverBackground": withAlpha(TOKYO_NIGHT.comment, 0x55),
			"diffEditor.insertedTextBackground": withAlpha(TOKYO.green, 0x33),
			"diffEditor.removedTextBackground": withAlpha(TOKYO.red, 0x33),
			"diffEditor.insertedLineBackground": withAlpha(TOKYO.green, 0x14),
			"diffEditor.removedLineBackground": withAlpha(TOKYO.red, 0x14),
			"diffEditorGutter.insertedLineBackground": withAlpha(TOKYO.green, 0x1f),
			"diffEditorGutter.removedLineBackground": withAlpha(TOKYO.red, 0x1f),
			"diffEditor.diagonalFill": TOKYO.line,
			"diffEditor.border": TOKYO.lineStrong,
			"diffEditor.unchangedRegionBackground": TOKYO.bg1,
			"diffEditor.unchangedRegionForeground": TOKYO.fg3,
			"diffEditor.unchangedCodeBackground": CLEAR,
			"editorOverviewRuler.border": CLEAR,
		},
	});
}

/** The options every read-only view shares (the editors demo's, on the page's mono face). */
export const VIEW_OPTIONS: monaco.editor.IEditorOptions & monaco.editor.IGlobalEditorOptions = {
	theme: "tokyo-night",
	readOnly: true,
	domReadOnly: true,
	fontFamily: MONO_FONT,
	fontSize: 12.5,
	lineHeight: LINE_HEIGHT,
	fontLigatures: false,
	minimap: { enabled: false },
	scrollBeyondLastLine: false,
	renderLineHighlight: "none",
	overviewRulerLanes: 0,
	hideCursorInOverviewRuler: true,
	scrollbar: {
		verticalScrollbarSize: 8,
		horizontalScrollbarSize: 8,
		useShadows: false,
		alwaysConsumeMouseWheel: false,
	},
	padding: { top: 6, bottom: 6 },
	guides: { indentation: true },
	automaticLayout: true,
	stickyScroll: { enabled: false },
	matchBrackets: "never",
	occurrencesHighlight: "off",
	selectionHighlight: false,
	contextmenu: false,
	tabSize: 2,
};

/** A model named by the file's path, so Monaco picks its language from the extension. */
export function fileModel(text: string, path: string, tag: string): monaco.editor.ITextModel {
	const uri = monaco.Uri.from({ scheme: "whiteboard", authority: tag, path: `/${path.replace(/^\/+/, "")}` });
	monaco.editor.getModel(uri)?.dispose();
	return monaco.editor.createModel(text, undefined, uri);
}
