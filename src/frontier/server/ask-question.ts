/**
 * The question an omp `ask` call puts to him, as one line (0589). Since omp 18.18.3 the call's arguments are one HTML
 * form, `{html}` (`tools/ask.ts:59-61`); before, `{questions: [{question, …}]}`. For a form the question is the first
 * named control's, worded as omp words it for its own blocked status (`modes/controllers/event-controller.ts:2168-2185`,
 * pi-tui `html-form/data.ts:127-146` `fieldQuestions`): the control's own label (a text field, number, select or
 * textarea wrapped in `<label>`, a `<label for>`, its `aria-label` or `title`; a radio or checkbox group has none),
 * else the text block just before it (`<p>`, a heading, …), else its fieldset's `<legend>`, else a lone checkbox's
 * label, else its name. Read with a tag scanner, not a DOM: the form is the model's plain semantic HTML
 * (`prompts/tools/ask.md`).
 */
import { asObject } from "../../shared/fsx.ts";

/** Elements whose content is never form text (pi-tui `html-form/parse.ts` `SKIPPED`, and pictures). */
const DROPPED = /<(script|style|template|button|head|noscript|iframe|title|svg)\b[\s\S]*?<\/\1\s*>|<!--[\s\S]*?-->/gi;
/** Elements that end a text block (`parse.ts` `BLOCK`, `HEADING`, `hr` and table cells). */
const BLOCK =
	/^(p|div|li|dd|dt|dl|section|article|aside|header|footer|main|nav|blockquote|details|figure|figcaption|address|hr|tr|td|th|h[1-6]|summary)$/;
/** Blocks that are context, not a question, unless they hold controls (`parse.ts` `#list`, `#table`, `pre`). */
const CONTEXT = /^(ul|ol|table|pre)$/;
const CONTROL = /^(input|select|textarea)$/;
const CONTROL_TAG = /<(input|select|textarea)\b/i;
/** `<input>` types that aren't fields (`parse.ts` `IGNORED_INPUTS`, and `hidden`). */
const NOT_A_FIELD = /^(hidden|submit|button|reset|image|file)$/;
const TAG = /<(\/?)\s*([a-z][a-z0-9]*)\b[^>]*>/gi;
const ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " };

function decode(text: string): string {
	return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, name: string) => {
		if (name[0] !== "#") return ENTITIES[name.toLowerCase()] ?? whole;
		const code = name[1] === "x" || name[1] === "X" ? Number.parseInt(name.slice(2), 16) : Number(name.slice(1));
		return Number.isInteger(code) && code >= 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole;
	});
}

/** Markup as one line of text: tags out, entities decoded, whitespace runs one space. */
function oneLine(markup: string): string {
	return decode(markup.replace(/<[^>]*>/g, " "))
		.replace(/\s+/g, " ")
		.trim();
}

/** A `<label>`'s text without its controls' own text (a select's options, a textarea's value; `parse.ts` `#spansOf`). */
function labelText(markup: string): string {
	return oneLine(markup.replace(/<(select|textarea)\b[\s\S]*?<\/\1\s*>/gi, " "));
}

/** A tag's attribute value, entities decoded; undefined when absent. */
function attribute(tag: string, name: string): string | undefined {
	const match = new RegExp(`\\s${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s>]+))`, "i").exec(tag);
	if (!match) return undefined;
	return decode(match[1] ?? match[2] ?? match[3] ?? "");
}

/** The form's first named control's question, as omp words it; null when the form has no named control. */
export function formQuestion(html: string): string | null {
	const doc = html.replace(DROPPED, " ");
	const lower = doc.toLowerCase();
	const until = (closer: string, from: number) => {
		const end = lower.indexOf(closer, from);
		return end < 0 ? doc.length : end;
	};
	const labelFor = new Map<string, string>();
	const names = new Map<string, number>();
	for (const tag of doc.matchAll(TAG)) {
		const name = tag[2]?.toLowerCase();
		if (name === "label" && tag[1] === "") {
			const id = attribute(tag[0], "for");
			const start = tag.index + tag[0].length;
			if (id && !labelFor.has(id)) labelFor.set(id, labelText(doc.slice(start, until("</label", start))));
		} else if (name && CONTROL.test(name) && tag[1] === "") {
			const control = attribute(tag[0], "name");
			if (control) names.set(control, (names.get(control) ?? 0) + 1);
		}
	}
	let legend = "";
	let caption = "";
	let pending = "";
	// The open `<label>`: where it ends and its text (its control's own label, or an option's).
	let label: { end: number; text: string } | undefined;
	let at = 0;
	const scanner = new RegExp(TAG.source, TAG.flags);
	for (let tag = scanner.exec(doc); tag; tag = scanner.exec(doc)) {
		if (!label) pending += doc.slice(at, tag.index);
		at = tag.index + tag[0].length;
		if (label && tag.index >= label.end) label = undefined;
		const closing = tag[1] === "/";
		const name = tag[2]?.toLowerCase() ?? "";
		if (name === "fieldset" || name === "legend") {
			if (name === "legend" && closing) legend = oneLine(pending);
			if (name === "fieldset") legend = "";
			pending = "";
			caption = "";
		} else if (name === "label" && !closing) {
			const end = until("</label", at);
			label = { end, text: labelText(doc.slice(at, end)) };
		} else if (CONTEXT.test(name) && !closing) {
			const end = until(`</${name}`, at);
			if (CONTROL_TAG.test(doc.slice(at, end))) {
				// A list or table of controls is layout: its text before it stays the caption.
				const text = oneLine(pending);
				if (text) caption = text;
			} else {
				// A list, table or preformatted block without controls is context, not a question.
				caption = "";
				at = end;
				scanner.lastIndex = end;
			}
			pending = "";
		} else if (name === "br") {
			pending += " ";
		} else if (name === "img" && !closing) {
			pending = "";
			caption = "";
		} else if (BLOCK.test(name)) {
			// A block's end (or start) ends the text before it: the last text block is the caption.
			const text = oneLine(pending);
			if (text) caption = text;
			pending = "";
		} else if (CONTROL.test(name) && !closing) {
			const type = name === "input" ? (attribute(tag[0], "type") ?? "text").toLowerCase() : name;
			if (NOT_A_FIELD.test(type)) continue;
			const text = oneLine(pending);
			if (text) caption = text;
			pending = "";
			const control = attribute(tag[0], "name");
			if (!control) {
				// An unnamed control is a field omp shows but `fieldQuestions` callers skip; it ends the caption.
				caption = "";
				continue;
			}
			const choice = type === "radio" || type === "checkbox";
			const id = attribute(tag[0], "id");
			const wrapping = choice ? "" : (label?.text ?? "");
			const detached =
				(id ? labelFor.get(id) : undefined) ??
				oneLine(attribute(tag[0], "aria-label") ?? attribute(tag[0], "title") ?? "");
			// A text field without a label is labelled by its name (`parse.ts` `#nameLabel`: `sentry_dsn` → `sentry dsn`).
			const own = choice ? "" : wrapping || detached || (name === "select" ? "" : control.replace(/[_-]+/g, " "));
			let lone = "";
			if (choice && names.get(control) === 1) lone = label?.text || attribute(tag[0], "value") || control;
			if (name === "select") {
				const options = doc.slice(at, until("</select", at)).match(/<option\b/gi) ?? [];
				if (options.length === 1) lone = oneLine(doc.slice(at, until("</select", at)));
			}
			return oneLine(own || caption || legend || lone || control);
		}
	}
	return null;
}

/**
 * The question of an `ask` call's arguments: omp ≥ 18.18.3's form (`{html}`), else the first of omp ≤ 18.18.2's
 * `questions[]`; null when neither holds one.
 */
export function askQuestion(args: unknown): string | null {
	const fields = asObject(args);
	if (typeof fields?.html === "string") return formQuestion(fields.html);
	const first = asObject(Array.isArray(fields?.questions) ? fields.questions[0] : undefined)?.question;
	return typeof first === "string" ? first : null;
}
