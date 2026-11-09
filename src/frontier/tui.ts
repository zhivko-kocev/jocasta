// The terminal programs' shared shell (0551, 0553): the `today` pane and the `ask` popup are OpenTUI programs that
// draw plain lines of coloured runs. This module owns what both need: the line model and its column math (display
// columns from `Bun.stringWidth`, so wide characters count twice), the styled text OpenTUI draws, and `runPane`, the one
// lifecycle: a renderer that owns no signal, a redraw on resize, draws that do nothing once torn down, and one teardown
// path (the program's cleanup, then the renderer destroyed and closed).
import {
	bg,
	bold,
	CliRenderEvents,
	createCliRenderer,
	decodePasteBytes,
	fg,
	type KeyEvent,
	StyledText,
	stripAnsiSequences,
	type TextChunk,
	TextRenderable,
} from "@opentui/core";
import { TOKYO_NIGHT } from "./canvas/palette.ts";

/** A run of text in one colour. */
export interface Segment {
	text: string;
	fg?: string;
	bg?: string;
	bold?: boolean;
}

export type Line = Segment[];

/** The columns `text` takes in a terminal: wide (CJK) characters take two. */
export function columns(text: string): number {
	return Bun.stringWidth(text);
}

/** The columns a line takes. */
export function lineColumns(line: Line): number {
	return line.reduce((n, s) => n + columns(s.text), 0);
}

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/** `text` split into pieces of at most `width` columns, every character kept (a wide one never split). */
export function chop(text: string, width: number): string[] {
	const out: string[] = [];
	let piece = "";
	let used = 0;
	for (const { segment } of graphemes.segment(text)) {
		const size = columns(segment);
		if (used > 0 && used + size > width) {
			out.push(piece);
			piece = "";
			used = 0;
		}
		piece += segment;
		used += size;
	}
	if (piece || out.length === 0) out.push(piece);
	return out;
}

/** Text cut to `width` columns, ending in "…" when it was longer. */
export function cut(text: string, width: number): string {
	if (width <= 0) return "";
	if (columns(text) <= width) return text;
	const head = width > 1 ? (chop(text, width - 1)[0] ?? "") : "";
	return columns(head) < width ? `${head}…` : "…";
}

/** A line's segments cut to `width` columns in all. */
export function fit(line: Line, width: number): Line {
	const out: Line = [];
	let left = width;
	for (const segment of line) {
		if (left <= 0) break;
		const text = cut(segment.text, left);
		out.push({ ...segment, text });
		left -= columns(text);
	}
	return out;
}

/** `text` wrapped at word boundaries to `width` columns; a word longer than the line is split, never cut. */
export function wrap(text: string, width: number): string[] {
	const out: string[] = [];
	for (const paragraph of text.split("\n")) {
		let line = "";
		let used = 0;
		for (const word of paragraph.split(/\s+/).filter(Boolean)) {
			const size = columns(word);
			if (line && used + 1 + size <= width) {
				line = `${line} ${word}`;
				used += 1 + size;
				continue;
			}
			if (line) out.push(line);
			const pieces = chop(word, width);
			out.push(...pieces.slice(0, -1));
			line = pieces.at(-1) ?? "";
			used = columns(line);
		}
		out.push(line);
	}
	return out;
}

/** The lines as OpenTUI styled text. */
export function styled(lines: Line[]): StyledText {
	const chunks: TextChunk[] = [];
	lines.forEach((line, index) => {
		if (index > 0) chunks.push(fg(TOKYO_NIGHT.fg)("\n"));
		for (const segment of line) {
			let chunk = fg(segment.fg ?? TOKYO_NIGHT.fg)(segment.text);
			if (segment.bold) chunk = bold(chunk);
			if (segment.bg) chunk = bg(segment.bg)(chunk);
			chunks.push(chunk);
		}
	});
	return new StyledText(chunks);
}

/** What a running program can ask of its pane. */
export interface Pane {
	/** Draws the program's lines now; nothing once the program is ending. */
	draw(): void;
	/** Ends the program with `code` (the first call counts). */
	end(code: number): void;
}

/** A program `runPane` runs. */
export interface PaneProgram {
	/** The lines for `width` × `height`. */
	lines(width: number, height: number): Line[];
	onKey?(key: KeyEvent): void;
	/** Pasted text, decoded and stripped of escape sequences (FE-11: a terminal's colour codes never reach a brief). */
	onPaste?(text: string): void;
	/** Runs once when the program ends, before the renderer goes: release held requests, close feeds, clear timers. */
	cleanup?(): void;
}

/**
 * The signals that end a pane: the renderer is created with `exitSignals: []`, so every catchable one comes here and
 * goes through the program's cleanup (FE-3: a popup that only destroyed its renderer kept its held request open).
 */
const END_SIGNALS: NodeJS.Signals[] =
	process.platform === "win32"
		? ["SIGINT", "SIGTERM", "SIGHUP", "SIGBREAK"]
		: ["SIGINT", "SIGTERM", "SIGHUP", "SIGQUIT", "SIGPIPE"];

/**
 * Runs one terminal program until it ends (its own `end`, a signal, or the renderer destroyed underneath it); resolves
 * its exit code once the renderer has closed. `setup` gets the pane and returns the program; it may draw at once.
 */
export async function runPane(setup: (pane: Pane) => PaneProgram): Promise<number> {
	const renderer = await createCliRenderer({
		exitOnCtrlC: false,
		exitSignals: [],
		useMouse: false,
		backgroundColor: TOKYO_NIGHT.bg0,
	});
	// No wrapping: a miscounted line clips at the edge instead of pushing the bottom rows off (FE-10).
	const text = new TextRenderable(renderer, { content: "", wrapMode: "none" });
	renderer.root.add(text);
	const { promise, resolve } = Promise.withResolvers<number>();
	let over = false;
	let program: PaneProgram | undefined;
	const pane: Pane = {
		draw() {
			if (over || !program) return;
			text.content = styled(program.lines(renderer.terminalWidth, renderer.terminalHeight));
			renderer.requestRender();
		},
		end(code) {
			if (over) return;
			over = true;
			resolve(code);
		},
	};
	const onSignal = () => pane.end(0);
	for (const signal of END_SIGNALS) process.on(signal, onSignal);
	renderer.on(CliRenderEvents.DESTROY, onSignal);
	renderer.on(CliRenderEvents.RESIZE, () => pane.draw());
	renderer.keyInput.on("keypress", (key) => {
		if (!over) program?.onKey?.(key);
	});
	renderer.keyInput.on("paste", (event) => {
		if (!over) program?.onPaste?.(stripAnsiSequences(decodePasteBytes(event.bytes)));
	});
	program = setup(pane);
	pane.draw();
	const code = await promise;
	for (const signal of END_SIGNALS) process.off(signal, onSignal);
	try {
		program.cleanup?.();
	} finally {
		renderer.destroy();
		await renderer.closed;
	}
	return code;
}
