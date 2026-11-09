// What the `today` pane draws (0551, 0555), as plain coloured lines from the service's feed: a header (date, time,
// weather, CPU and memory, quota, today's spend), NEEDS YOU, UNDERWAY (thread, state, time, cost, context), LANDED
// TODAY, NEXT (the calendar), NEWS while rows remain, and a key line. No greeting, no Presence. Tokyo Night from the
// one palette file (0474). Pure: `main.ts` turns the lines into OpenTUI text.
import { TOKYO_NIGHT } from "../canvas/palette.ts";
import { type Dashboard, type Hud, quotaHigh, quotaPercent, type Today, waitLabel } from "../protocol.ts";

/** A run of text in one colour. */
export interface Segment {
	text: string;
	fg?: string;
	bg?: string;
	bold?: boolean;
}

export type Line = Segment[];

/** What the pane knows: the feed's last figures, whether it is connected, the picked thread and his last key's outcome. */
export interface TodayState {
	up: boolean;
	hud?: Hud;
	today?: Today;
	dashboard?: Dashboard;
	/** The picked thread's pane. */
	selected?: string;
	/** What his last key did, or why not. */
	said?: { text: string; isError: boolean };
}

const P = TOKYO_NIGHT;

/** The pane's key line (0555). */
export const KEY_LINE = "↑↓ thread · Enter go to it · f Finish · s Stop";

/** The threads `today` lists, in its order, by pane: up/down move between them. */
export function threadPanes(today: Today | undefined): string[] {
	if (!today) return [];
	const panes = [
		...today.needsYou.flatMap((n) => (n.paneId ? [n.paneId] : [])),
		...today.underway.map((u) => u.paneId),
		...today.landed.flatMap((l) => (l.paneId ? [l.paneId] : [])),
	];
	return [...new Set(panes)];
}

/** Text cut to `width` columns, ending in "…" when it was longer. */
function cut(text: string, width: number): string {
	if (width <= 0) return "";
	return text.length <= width ? text : `${text.slice(0, Math.max(0, width - 1))}…`;
}

/** A line's segments cut to `width` in all. */
function fit(line: Line, width: number): Line {
	const out: Line = [];
	let left = width;
	for (const segment of line) {
		if (left <= 0) break;
		const text = cut(segment.text, left);
		out.push({ ...segment, text });
		left -= text.length;
	}
	return out;
}

/** `left` and `right` on one line, `right` flush to the edge; `left` cut first when they don't fit. */
function spread(left: Line, right: Line, width: number): Line {
	const rightWidth = right.reduce((n, s) => n + s.text.length, 0);
	const room = width - rightWidth - 1;
	if (rightWidth === 0 || room < 8) return fit(left, width);
	const shown = fit(left, room);
	const used = shown.reduce((n, s) => n + s.text.length, 0);
	return [...shown, { text: " ".repeat(width - used - rightWidth) }, ...right];
}

/** Parts joined with " · ", flowed over as many lines as `width` needs. */
function flow(parts: Line[], width: number): Line[] {
	const lines: Line[] = [];
	let line: Line = [];
	let used = 0;
	for (const part of parts) {
		const size = part.reduce((n, s) => n + s.text.length, 0);
		if (used > 0 && used + 3 + size > width) {
			lines.push(line);
			line = [];
			used = 0;
		}
		if (used > 0) {
			line.push({ text: " · ", fg: P.comment });
			used += 3;
		}
		line.push(...part);
		used += size;
	}
	if (line.length > 0) lines.push(line);
	return lines.map((l) => fit(l, width));
}

/** `$3.20`. */
const money = (usd: number) => `$${usd.toFixed(2)}`;

/** Context tokens, short: `83k`, `1.2M`. */
export function tokens(n: number): string {
	if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
	if (n >= 1_000) return `${Math.round(n / 1_000)}k`;
	return String(n);
}

/** The header (0555): date and time, weather, CPU and memory, the quota windows and today's spend. */
function header(state: TodayState, now: Date, width: number): Line[] {
	const date = now.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
	const time = now.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
	const parts: Line[] = [[{ text: `${date} ${time}`, fg: P.fg, bold: true }]];
	const weather = state.dashboard?.weather;
	if (weather) parts.push([{ text: `${Math.round(weather.tempC)}° ${weather.summary}`, fg: P.cyan }]);
	const machine = state.dashboard?.machine;
	if (machine) {
		const mem = machine.ram.total > 0 ? Math.round((machine.ram.used / machine.ram.total) * 100) : 0;
		parts.push([{ text: `cpu ${Math.round(machine.cpu * 100)}% · mem ${mem}%`, fg: P.fg2 }]);
	}
	for (const q of state.hud?.quota ?? []) {
		parts.push([{ text: `${q.window} ${quotaPercent(q)}%`, fg: quotaHigh(q) ? P.red : P.fg2 }]);
	}
	const spend = state.hud?.spendToday;
	parts.push([{ text: spend === null || spend === undefined ? "… today" : `${money(spend)} today`, fg: P.green }]);
	return flow(parts, width);
}

const heading = (text: string): Line => [{ text, fg: P.blue, bold: true }];
const quiet = (text: string): Line => [{ text: `  ${text}`, fg: P.comment }];

/** A thread row: picked ones on the selection colour, with a marker. */
function row(
	line: Line,
	paneId: string | undefined,
	state: TodayState,
	width: number,
): { line: Line; paneId?: string } {
	const picked = paneId !== undefined && paneId === state.selected;
	const marked: Line = [{ text: picked ? "› " : "  ", fg: P.blue }, ...line];
	const fitted = fit(marked, width);
	const used = fitted.reduce((n, s) => n + s.text.length, 0);
	const filled = picked
		? [...fitted, { text: " ".repeat(Math.max(0, width - used)) }].map((s) => ({ ...s, bg: P.bg2 }))
		: fitted;
	return paneId === undefined ? { line: filled } : { line: filled, paneId };
}

/** The sections above NEWS, each line with the thread it names (for the picked row and scrolling). */
function body(state: TodayState, now: Date, width: number): { line: Line; paneId?: string }[] {
	const today = state.today;
	const out: { line: Line; paneId?: string }[] = [];
	if (!today) return out;
	out.push({ line: heading("NEEDS YOU") });
	if (today.needsYou.length === 0) out.push({ line: quiet("nothing waits on you") });
	for (const n of today.needsYou) {
		const colour = n.kind === "blocked" ? P.red : n.kind === "reviewed" ? P.orange : P.yellow;
		const text = `${n.time ? `${n.time} ` : ""}${n.project && n.paneId ? `${n.project}/` : ""}${n.text}`;
		const right: Line = n.since ? [{ text: waitLabel(now.getTime() - Date.parse(n.since)), fg: P.comment }] : [];
		out.push(row(spread([{ text, fg: colour }], right, width - 2), n.paneId, state, width));
	}
	out.push({ line: heading("UNDERWAY") });
	if (today.underway.length === 0) out.push({ line: quiet("no thread at work") });
	for (const u of today.underway) {
		const facts: Line = [
			{ text: u.state ?? "working", fg: u.state === "idle" ? P.comment : P.green },
			{ text: u.since ? `  ${waitLabel(now.getTime() - u.since)}` : "", fg: P.fg2 },
			{ text: u.cost !== undefined ? `  ${money(u.cost)}` : "", fg: P.fg2 },
			{ text: u.context !== undefined ? `  ${tokens(u.context)}` : "", fg: P.purple },
		];
		const name = `${u.project ? `${u.project}/` : ""}${u.title}`;
		out.push(row(spread([{ text: name, fg: P.fg }], facts, width - 2), u.paneId, state, width));
	}
	if (today.landed.length > 0) {
		out.push({ line: heading("LANDED TODAY") });
		for (const l of today.landed) {
			const text = `${l.project ? `${l.project}/` : ""}${l.title}${l.paneId ? "" : ` (${l.line})`}`;
			out.push(row([{ text, fg: l.paneId ? P.fg : P.fg2 }], l.paneId, state, width));
		}
	}
	const hhmm = now.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
	const next = today.calendar.filter((c) => c.time === "all day" || c.time >= hhmm);
	out.push({ line: heading("NEXT") });
	if (next.length === 0) out.push({ line: quiet("nothing more on the calendar today") });
	for (const c of next) {
		out.push({
			line: fit(
				[
					{ text: `  ${c.time} `, fg: P.cyan },
					{ text: c.title, fg: P.fg },
				],
				width,
			),
		});
	}
	return out;
}

/**
 * The pane's lines for `width` × `height`: the header, the sections (scrolled so the picked thread shows when they
 * don't fit), NEWS in whatever rows are left, then his last key's outcome and the key line.
 */
export function render(state: TodayState, now: Date, width: number, height: number): Line[] {
	const bottom: Line[] = [];
	if (state.said) {
		bottom.push(fit([{ text: state.said.text, fg: state.said.isError ? P.red : P.green }], width));
	}
	bottom.push(fit([{ text: KEY_LINE, fg: P.comment }], width));
	const top = state.up
		? header(state, now, width)
		: [fit([{ text: "Jocasta's service isn't answering; trying again…", fg: P.yellow }], width)];
	top.push([]);
	const room = Math.max(0, height - top.length - bottom.length);
	let rows = body(state, now, width);
	if (rows.length > room) {
		const picked = rows.findIndex((r) => r.paneId !== undefined && r.paneId === state.selected);
		const start = picked < 0 ? 0 : Math.min(Math.max(0, picked - Math.floor(room / 2)), rows.length - room);
		rows = rows.slice(start, start + room);
	}
	const lines = [...top, ...rows.map((r) => r.line)];
	const left = height - lines.length - bottom.length;
	const news = state.dashboard?.news.flatMap((feed) => feed.items.map((item) => ({ feed: feed.heading, item }))) ?? [];
	if (left >= 3 && news.length > 0) {
		lines.push([], heading("NEWS"));
		for (const { feed, item } of news.slice(0, left - 2)) {
			lines.push(
				fit(
					[
						{ text: `  ${feed}: `, fg: P.comment },
						{ text: item.title, fg: P.fg2 },
					],
					width,
				),
			);
		}
	}
	while (lines.length < height - bottom.length) lines.push([]);
	return [...lines.slice(0, Math.max(0, height - bottom.length)), ...bottom].slice(0, height);
}
