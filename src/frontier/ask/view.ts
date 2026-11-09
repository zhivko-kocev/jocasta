// What the `ask` popup shows and how his keys answer it (0553, 0558, 0563, 0568), pure so it is tested apart from
// OpenTUI: the arrows or Tab pick a choice; a coded question's choices other than its fallback (Allow, Merge, PR, Stop
// everything) take its three letters typed, never Enter; the fallback takes Enter when picked, and Esc always; New
// thread picks a project and takes a brief. `main.ts` draws the lines and posts the answer.
import { TOKYO_NIGHT } from "../canvas/palette.ts";
import type { PopupAnswer, PopupView } from "../protocol.ts";
import type { Line } from "../today/view.ts";

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
	/** One line under the question: a wrong code, a refusal. */
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
		field: view.form?.project ? "brief" : "project",
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

/** Applies one key; resolves the answer it gives, if any. */
export function press(state: AskState, key: Key): PopupAnswer | undefined {
	const { view } = state;
	if (key.name === "escape" || (key.ctrl && key.name === "c")) return { choice: view.fallback };
	if (view.form) {
		const projects = view.form.projects;
		if (key.name === "tab") {
			state.field = state.field === "project" ? "brief" : "project";
			return undefined;
		}
		if (state.field === "project") {
			if (key.name === "up") state.project = (state.project + projects.length - 1) % projects.length;
			else if (key.name === "down") state.project = (state.project + 1) % projects.length;
			else if (key.name === "return" || key.name === "enter") state.field = "brief";
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
		}
	}
	return undefined;
}

/** Pasted text: into New thread's brief, one line. */
export function paste(state: AskState, text: string): void {
	if (state.view.form && state.field === "brief") state.brief += text.replace(/\s+/g, " ");
}

/** `text` wrapped at word boundaries to `width`. */
export function wrap(text: string, width: number): string[] {
	const out: string[] = [];
	for (const paragraph of text.split("\n")) {
		let line = "";
		for (const word of paragraph.split(/\s+/).filter(Boolean)) {
			if (line && line.length + 1 + word.length > width) {
				out.push(line);
				line = "";
			}
			line = line ? `${line} ${word}` : word.length > width ? word.slice(0, width) : word;
		}
		out.push(line);
	}
	return out;
}

const P = TOKYO_NIGHT;

/** The popup's lines for `width` columns. */
export function askLines(state: AskState, width: number): Line[] {
	const { view } = state;
	const lines: Line[] = [[{ text: view.title, fg: P.blue, bold: true }], []];
	for (const l of wrap(view.message, width)) lines.push([{ text: l, fg: P.fg }]);
	if (view.detail) {
		lines.push([]);
		for (const l of wrap(view.detail, width)) lines.push([{ text: l, fg: P.fg2 }]);
	}
	lines.push([]);
	if (view.form) {
		const project = view.form.projects[state.project] ?? "";
		const on = (field: AskState["field"]) => (state.field === field ? { bg: P.bg2 } : {});
		lines.push([
			{ text: "Project  ", fg: P.comment },
			{ text: `‹ ${project} ›`, fg: P.cyan, bold: true, ...on("project") },
		]);
		const room = Math.max(10, width - 9);
		const brief = state.brief.length > room - 1 ? `…${state.brief.slice(-(room - 2))}` : state.brief;
		lines.push([
			{ text: "Brief    ", fg: P.comment },
			{ text: `${brief}▏`, fg: P.fg, ...on("brief") },
		]);
		lines.push([]);
		lines.push([{ text: "Tab field · ↑↓ project · Enter start · Esc cancel", fg: P.comment }]);
	} else {
		const buttons: Line = [];
		view.choices.forEach((choice, index) => {
			if (index > 0) buttons.push({ text: "  " });
			buttons.push(
				index === state.picked
					? { text: ` ${choice} `, fg: P.bg0, bg: P.blue, bold: true }
					: { text: ` ${choice} `, fg: P.fg, bg: P.bg2 },
			);
		});
		lines.push(buttons);
		lines.push([]);
		const picked = view.choices[state.picked] as string;
		if (coded(state) && view.code !== undefined) {
			lines.push([
				{ text: "Type ", fg: P.fg },
				{ text: view.code, fg: P.yellow, bold: true },
				{ text: ` for ${picked}`, fg: P.fg },
			]);
			lines.push([
				{ text: "> ", fg: P.comment },
				{ text: state.typed.padEnd(view.code.length, "_"), fg: P.yellow, bold: true },
			]);
			lines.push([]);
			lines.push([{ text: `←→ choose · Esc ${view.fallback}`, fg: P.comment }]);
		} else {
			lines.push([{ text: `←→ choose · Enter ${picked} · Esc ${view.fallback}`, fg: P.comment }]);
		}
	}
	if (state.note) lines.push([], [{ text: state.note, fg: P.red }]);
	return lines;
}
