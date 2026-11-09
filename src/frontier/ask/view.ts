// What the `ask` popup shows and how his keys answer it (0553, 0558, 0563, 0568, 0578), pure so it is tested apart
// from OpenTUI: the arrows or Tab pick a choice; a coded question's choices other than its fallback (Allow, Merge, PR,
// Stop everything) take its three letters typed, never Enter; the fallback takes Enter when picked, and Esc always;
// PgUp/PgDn read a question taller than the popup; New thread picks a project and takes a brief. `main.ts` draws the
// lines and posts the answer.
import { TOKYO_NIGHT } from "../canvas/palette.ts";
import type { PopupAnswer, PopupView } from "../protocol.ts";
import { columns, fit, type Line, wrap } from "../tui.ts";

export interface AskState {
	view: PopupView;
	/** The picked choice's index: a coded question's first choice at first, otherwise the fallback's. */
	picked: number;
	/** The code's letters typed so far for the picked choice. */
	typed: string;
	/** New thread: the picked project's index, the brief, and the field the keys go to. */
	project: number;
	brief: string;
	field: "project" | "brief";
	/** The first line of the question's text shown, when it is taller than the popup (PgUp/PgDn move it). */
	scroll: number;
	/** One line above the hint: a wrong code, a refusal. */
	note?: string;
}

/** The key as OpenTUI reports it, the fields read here. */
export interface Key {
	name: string;
	sequence: string;
	ctrl?: boolean;
	meta?: boolean;
	shift?: boolean;
}

export function askState(view: PopupView): AskState {
	const projects = view.form?.projects ?? [];
	const project = view.form?.project ? projects.indexOf(view.form.project) : 0;
	return {
		view,
		picked: view.code !== undefined ? 0 : Math.max(0, view.choices.indexOf(view.fallback)),
		typed: "",
		project: Math.max(0, project),
		brief: "",
		// The form opens on the brief, with a project always picked (0581, UX-15).
		field: "brief",
		scroll: 0,
	};
}

/** A printable character the key types, or undefined. */
function typedChar(key: Key): string | undefined {
	if (key.ctrl || key.meta) return undefined;
	const char = key.sequence;
	return char.length === 1 && char >= " " && char !== "\x7f" ? char : undefined;
}

/** Whether `state`'s picked choice takes the code. */
function coded(state: AskState): boolean {
	return state.view.code !== undefined && state.view.choices[state.picked] !== state.view.fallback;
}

/** The note for a letter typed on a coded choice that isn't A–Z, such as a Cyrillic layout's (0578). */
export function latinNote(code: string): string {
	return `Type the Latin letters ${code} (switch the keyboard layout).`;
}

/** The note after a wrong code (0578): a warning when one try is left, `fallback` being what the third one takes. */
export function wrongNote(left: number | undefined, fallback: string): string {
	return left === 1 ? `Wrong code; one try left, then it counts as ${fallback}.` : "Wrong code; nothing was done.";
}

/** The note when the third wrong code took the fallback (0578), shown in red before the popup closes. */
export function exhaustedNote(fallback: string): string {
	return `Wrong code three times; ${fallback} was taken.`;
}

/** Applies one key; resolves the answer it gives, if any. */
export function press(state: AskState, key: Key): PopupAnswer | undefined {
	const { view } = state;
	if (key.name === "escape" || (key.ctrl && key.name === "c")) return { choice: view.fallback };
	if (key.name === "pageup" || key.name === "pagedown") {
		// The layout clamps it to the text's length.
		state.scroll = Math.max(0, state.scroll + (key.name === "pageup" ? -1 : 1) * PAGE);
		return undefined;
	}
	if (view.form) {
		const projects = view.form.projects;
		if (key.name === "tab") {
			state.field = state.field === "project" ? "brief" : "project";
			return undefined;
		}
		if (state.field === "project") {
			const char = typedChar(key);
			if (key.name === "up") state.project = (state.project + projects.length - 1) % projects.length;
			else if (key.name === "down") state.project = (state.project + 1) % projects.length;
			else if (key.name === "return" || key.name === "enter") state.field = "brief";
			else if (char && char !== " ") {
				// Type-to-find (UX-15): the next project, after the picked one, that starts with the letter.
				for (let step = 1; step <= projects.length; step++) {
					const at = (state.project + step) % projects.length;
					if (projects[at]?.toLowerCase().startsWith(char.toLowerCase())) {
						state.project = at;
						break;
					}
				}
			}
			return undefined;
		}
		if (key.name === "backspace") state.brief = state.brief.slice(0, -1);
		else if (key.name === "return" || key.name === "enter") {
			const brief = state.brief.trim();
			if (!brief) {
				state.note = "Write the brief first.";
				return undefined;
			}
			return { choice: view.choices[0] as string, project: projects[state.project] as string, brief };
		} else {
			const char = typedChar(key);
			if (char) state.brief += char;
		}
		return undefined;
	}
	const count = view.choices.length;
	const choice = view.choices[state.picked] as string;
	if (key.name === "left" || key.name === "up" || (key.name === "tab" && key.shift)) {
		state.picked = (state.picked + count - 1) % count;
		state.typed = "";
	} else if (key.name === "right" || key.name === "down" || key.name === "tab") {
		state.picked = (state.picked + 1) % count;
		state.typed = "";
	} else if (key.name === "return" || key.name === "enter") {
		if (!coded(state)) return { choice };
	} else if (coded(state) && view.code !== undefined) {
		if (key.name === "backspace") state.typed = state.typed.slice(0, -1);
		const char = typedChar(key);
		if (char && /^[a-z]$/i.test(char) && state.typed.length < view.code.length) {
			state.typed += char.toUpperCase();
			state.note = undefined;
			if (state.typed.length === view.code.length) return { choice, typed: state.typed };
		} else if (char && /\p{L}/u.test(char)) {
			state.note = latinNote(view.code);
		}
	}
	return undefined;
}

/** Pasted text (already stripped of escape sequences): into New thread's brief, one line. */
export function paste(state: AskState, text: string): void {
	if (state.view.form && state.field === "brief") state.brief += text.replace(/\s+/g, " ");
}

const P = TOKYO_NIGHT;

/** Lines PgUp and PgDn move the question's text by. */
const PAGE = 5;

/** A hint's parts joined with " · " in the separator colour (0581, UX-12). */
function hint(parts: string[]): Line {
	return parts.flatMap((text, i) => [...(i > 0 ? [{ text: " · ", fg: P.comment }] : []), { text, fg: P.fg3 }]);
}

/**
 * The block at the popup's foot, laid out first so it always shows (0578): the choices or the form, the code to type
 * and the letters typed, the note, and the key hint (`read` when the question's text scrolls).
 */
function foot(state: AskState, width: number, read: boolean): Line[] {
	const { view } = state;
	const lines: Line[] = [];
	const note: Line[] = state.note ? [[{ text: state.note, fg: P.red }]] : [];
	const reading = read ? ["PgUp/PgDn read"] : [];
	if (view.form) {
		const project = view.form.projects[state.project] ?? "";
		const on = (field: AskState["field"]) => (state.field === field ? { bg: P.bg2 } : {});
		lines.push([
			{ text: "Project  ", fg: P.fg3 },
			{ text: `‹ ${project} ›`, fg: P.cyan, bold: true, ...on("project") },
		]);
		// The brief's tail, so the caret's end shows; the caret only in the field the keys go to (UX-15).
		const caret = state.field === "brief" ? "▏" : "";
		const room = Math.max(10, width - 9) - columns(caret);
		let brief = state.brief;
		if (columns(brief) > room) {
			while (columns(brief) > room - 1) brief = brief.slice(1);
			brief = `…${brief}`;
		}
		lines.push([
			{ text: "Brief    ", fg: P.fg3 },
			{ text: `${brief}${caret}`, fg: P.fg, ...on("brief") },
		]);
		lines.push([], ...note);
		lines.push(
			hint(
				state.field === "brief"
					? ["Tab project", "Enter start", ...reading, "Esc cancel"]
					: ["Type to find", "↑↓ project", "Enter brief", ...reading, "Esc cancel"],
			),
		);
		return lines.map((l) => fit(l, width));
	}
	const buttons: Line = [];
	view.choices.forEach((choice, index) => {
		if (index > 0) buttons.push({ text: "  " });
		buttons.push(
			index === state.picked
				? { text: ` ${choice} `, fg: P.bg0, bg: P.blue, bold: true }
				: { text: ` ${choice} `, fg: P.fg, bg: P.bg2 },
		);
	});
	lines.push(buttons, []);
	const picked = view.choices[state.picked] as string;
	if (coded(state) && view.code !== undefined) {
		lines.push([
			{ text: "Type ", fg: P.fg },
			{ text: view.code, fg: P.yellow, bold: true },
			{ text: ` for ${picked}`, fg: P.fg },
		]);
		lines.push([
			{ text: "> ", fg: P.fg3 },
			{ text: state.typed.padEnd(view.code.length, "_"), fg: P.yellow, bold: true },
		]);
		lines.push(...note, []);
		lines.push(hint(["←→ choose", ...reading, `Esc ${view.fallback}`]));
	} else {
		if (note.length > 0) lines.push(...note, []);
		lines.push(hint(["←→ choose", `Enter ${picked}`, ...reading, `Esc ${view.fallback}`]));
	}
	return lines.map((l) => fit(l, width));
}

/**
 * The popup's lines for `width` × `height` (0578): the foot first, then the title, and the message and detail in the
 * rows left. A text taller than that shows from `state.scroll` (clamped here) and ends with `… N more lines · PgDn reads
 * on`; long words are split, never cut.
 */
export function askLines(state: AskState, width: number, height: number): Line[] {
	const { view } = state;
	const text: Line[] = wrap(view.message, width).map((l) => [{ text: l, fg: P.fg }]);
	if (view.detail) {
		text.push([]);
		for (const l of wrap(view.detail, width)) text.push([{ text: l, fg: P.fg2 }]);
	}
	const head: Line[] = [fit([{ text: view.title, fg: P.blue, bold: true }], width), []];
	let bottom = foot(state, width, false);
	// The rows for the text: everything but the title, its blank line, the blank line above the foot and the foot.
	let room = height - head.length - 1 - bottom.length;
	if (text.length > room) {
		bottom = foot(state, width, true);
		room = height - head.length - 1 - bottom.length;
	}
	let shown = text;
	if (text.length > room) {
		const last = Math.max(0, text.length - room);
		state.scroll = Math.min(state.scroll, last);
		if (state.scroll >= last) shown = text.slice(last);
		else {
			const rows = Math.max(0, room - 1);
			const more = text.length - state.scroll - rows;
			shown = [
				...text.slice(state.scroll, state.scroll + rows),
				fit(
					[
						{ text: `… ${more} more ${more === 1 ? "line" : "lines"}`, fg: P.fg3 },
						{ text: " · ", fg: P.comment },
						{ text: "PgDn reads on", fg: P.fg3 },
					],
					width,
				),
			];
		}
	} else state.scroll = 0;
	const lines = [...head, ...shown, [], ...bottom];
	// Too short even for the foot: the foot wins (the code he must type, then the hint).
	return lines.length > height ? lines.slice(lines.length - Math.max(0, height)) : lines;
}
