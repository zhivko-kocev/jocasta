// What the `today` pane draws (0551, 0555, 0581), as plain coloured lines from the service's feed: a header (date,
// time, quiet hours, a broken service, weather, CPU and memory, quota, today's spend), NEEDS YOU, UNDERWAY (thread,
// state, time, cost, context), LANDED TODAY, CALENDAR, then NEXT (his notes) and NEWS while rows remain, his last key's
// outcome and a key line. No greeting, no Presence. Tokyo Night from the one palette file (0474). Pure: `main.ts` draws
// the lines through the shared shell (`../tui.ts`).
import { dayLabel, hhmm } from "../../shared/day.ts";
import { TOKYO_NIGHT } from "../canvas/palette.ts";
import { type Dashboard, type Hud, quotaHigh, quotaPercent, type Today, waitLabel } from "../protocol.ts";
import { columns, cut, fit, type Line, lineColumns, type Segment, wrap } from "../tui.ts";

/** What the pane knows: the feed's last figures, whether it is connected, the picked row and his last key's outcome. */
export interface TodayState {
	up: boolean;
	/** Whether the feed has connected since the pane opened; until then it says it is connecting (UX-17). */
	seen?: boolean;
	/** When the feed went down after being up (epoch ms). */
	downSince?: number;
	hud?: Hud;
	today?: Today;
	dashboard?: Dashboard;
	/** The picked row's key: a thread's pane id, or a link row's URL (0581). */
	selected?: string;
	/** What his last key did, or why not, and when (epoch ms): it shows for `SAID_MS`. */
	said?: { text: string; isError: boolean; at: number };
}

/** A row Enter acts on (0581): a thread's pane (Enter goes to it) or a link (Enter opens it). */
export interface Pick {
	key: string;
	paneId?: string;
	url?: string;
}

/** What `render` draws: the lines, and the pickable rows in their drawn order (up/down move between them). */
export interface Frame {
	lines: Line[];
	picks: Pick[];
}

const P = TOKYO_NIGHT;

/** His last key's outcome shows this long (0581, UX-5). */
export const SAID_MS = 60_000;

/** The pane's keys, most important first: the key line drops the last ones that don't fit (0581, UX-7, UX-16). */
const KEYS = ["↑↓ pick", "Enter go/open", "f Finish", "s Stop", "r Review", "n New"];
export const KEY_LINE = KEYS.join(" · ");

/** The title of a row keeps at least this many columns before its facts give way (0581, UX-4). */
const MIN_TITLE = 18;

/** Text parts joined with " · ": the parts in quiet text, the separators in comment (0581, UX-12). */
function hint(parts: string[]): Line {
	return parts.flatMap((text, i) => [...(i > 0 ? [{ text: " · ", fg: P.comment }] : []), { text, fg: P.fg3 }]);
}

/** The key line for `width`: lower keys dropped while it doesn't fit. */
function keyLine(width: number): Line {
	let count = KEYS.length;
	while (count > 1 && columns(KEYS.slice(0, count).join(" · ")) > width) count--;
	return fit(hint(KEYS.slice(0, count)), width);
}

/**
 * `left` and the facts on one line, the facts flush to the edge. While the left part would get fewer than
 * `MIN_TITLE` columns (or its own width, when shorter), the last fact drops; the left part is cut after that. `tail`
 * (a link's ↗) follows the left part and is never cut.
 */
function spread(left: Line, facts: Segment[], width: number, tail?: Segment): Line {
	const tails: Line = tail ? [tail] : [];
	const want = Math.min(lineColumns(left) + lineColumns(tails), MIN_TITLE);
	let kept = facts.filter((s) => s.text);
	while (kept.length > 0 && width - lineColumns(kept) - 1 < want) kept = kept.slice(0, -1);
	const room = kept.length > 0 ? width - lineColumns(kept) - 1 : width;
	const shown = [...fit(left, room - lineColumns(tails)), ...tails];
	if (kept.length === 0) return fit(shown, width);
	return [...shown, { text: " ".repeat(Math.max(1, width - lineColumns(shown) - lineColumns(kept))) }, ...kept];
}

/** Parts joined with " · ", flowed over as many lines as `width` needs. */
function flow(parts: Line[], width: number): Line[] {
	const lines: Line[] = [];
	let line: Line = [];
	let used = 0;
	for (const part of parts) {
		const size = lineColumns(part);
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

/**
 * The header (0555, 0581): date and time, `quiet` during quiet hours, each broken service in red with its note, the
 * weather, CPU and memory, the quota windows (a high one with its reset when that is within a day) and today's spend.
 * The weather, then CPU and memory, give way when the header would pass two rows.
 */
function header(state: TodayState, now: Date, width: number): Line[] {
	const date = dayLabel(now);
	const parts: { line: Line; yields?: number }[] = [{ line: [{ text: `${date} ${hhmm(now)}`, fg: P.fg, bold: true }] }];
	if (state.hud?.quiet) parts.push({ line: [{ text: "quiet", fg: P.fg3 }] });
	for (const service of state.dashboard?.services ?? []) {
		if (service.state !== "down") continue;
		parts.push({ line: [{ text: `${service.name} down${service.note ? ` (${service.note})` : ""}`, fg: P.red }] });
	}
	const weather = state.dashboard?.weather;
	if (weather)
		parts.push({ line: [{ text: `${Math.round(weather.tempC)}° ${weather.summary}`, fg: P.cyan }], yields: 1 });
	const machine = state.dashboard?.machine;
	if (machine) {
		const mem = machine.ram.total > 0 ? Math.round((machine.ram.used / machine.ram.total) * 100) : 0;
		parts.push({ line: [{ text: `cpu ${Math.round(machine.cpu * 100)}% · mem ${mem}%`, fg: P.fg2 }], yields: 2 });
	}
	for (const q of state.hud?.quota ?? []) {
		const high = quotaHigh(q);
		const resets = q.resetsAt ? Date.parse(q.resetsAt) - now.getTime() : Number.NaN;
		const reset = high && resets > 0 && resets <= 86_400_000 ? ` (${hhmm(Date.parse(q.resetsAt ?? ""))})` : "";
		parts.push({ line: [{ text: `${q.window} ${quotaPercent(q)}%${reset}`, fg: high ? P.red : P.fg2 }] });
	}
	const spend = state.hud?.spendToday;
	parts.push({
		line: [{ text: spend === null || spend === undefined ? "… today" : `${money(spend)} today`, fg: P.green }],
	});
	let lines = flow(
		parts.map((p) => p.line),
		width,
	);
	for (const level of [1, 2]) {
		if (lines.length <= 2) break;
		lines = flow(
			parts.filter((p) => p.yields === undefined || p.yields > level).map((p) => p.line),
			width,
		);
	}
	return lines;
}

/** The header's place while the feed isn't up (UX-17): connecting at first, then the time and since when it is down. */
function offline(state: TodayState, now: Date, width: number): Line {
	if (!state.seen) return fit([{ text: "Connecting to Jocasta…", fg: P.fg3 }], width);
	const since = state.downSince === undefined ? "" : ` (since ${hhmm(state.downSince)})`;
	return fit(
		[
			{ text: hhmm(now), fg: P.fg, bold: true },
			{ text: " · ", fg: P.comment },
			{ text: `Jocasta isn't answering${since}; trying again…`, fg: P.yellow },
		],
		width,
	);
}

const heading = (text: string, width: number): Line => fit([{ text, fg: P.blue, bold: true }], width);

/** An empty section's line, in quiet text. */
const quiet = (parts: string[], width: number): Line => fit([{ text: "  " }, ...hint(parts)], width);

/** A body row; `pick` when Enter acts on it. */
interface Row {
	line: Line;
	pick?: Pick;
}

/** A pickable row: the picked one carries the marker and the selection colour across the pane. */
function row(line: Line, pick: Pick | undefined, state: TodayState, width: number): Row {
	const picked = pick !== undefined && pick.key === state.selected;
	const fitted = fit([{ text: picked ? "› " : "  ", fg: P.blue }, ...line], width);
	const filled = picked
		? [...fitted, { text: " ".repeat(Math.max(0, width - lineColumns(fitted))) }].map((s) => ({ ...s, bg: P.bg2 }))
		: fitted;
	return pick ? { line: filled, pick } : { line: filled };
}

/** What a row with a link shows after its text (0581, UX-7): Enter opens it. */
const LINK: Segment = { text: " ↗", fg: P.blue };

/** A NEEDS YOU row's kind (0581, UX-4): `?` a question or plan, `✓` reviewed, `↗` a review request, `!` red. */
const GLYPHS: Record<Today["needsYou"][number]["kind"], string> = {
	blocked: "?",
	reviewed: "✓",
	review: "↗",
	pipeline: "!",
	disk: "!",
	reminder: "",
};

function pickOf(paneId: string | undefined, url: string | undefined): Pick | undefined {
	if (paneId) return { key: paneId, paneId };
	return url ? { key: url, url } : undefined;
}

/** The sections above NEXT and NEWS, each row with what Enter does on it (for the picked row and scrolling). */
function body(state: TodayState, now: Date, width: number): Row[] {
	const today = state.today;
	const out: Row[] = [];
	if (!today) return out;
	out.push({ line: heading("NEEDS YOU", width) });
	if (today.needsYou.length === 0) out.push({ line: quiet(["nothing waits on you"], width) });
	for (const n of today.needsYou) {
		const colour = n.kind === "blocked" ? P.red : n.kind === "reviewed" ? P.orange : P.yellow;
		// A reminder keeps its time where the others have their kind's glyph.
		const lead = n.time ?? GLYPHS[n.kind];
		const text = `${lead ? `${lead} ` : ""}${n.project && n.paneId ? `${n.project}/` : ""}${n.text}`;
		const wait: Segment[] = n.since ? [{ text: waitLabel(now.getTime() - Date.parse(n.since)), fg: P.fg3 }] : [];
		const pick = pickOf(n.paneId, n.url);
		// A review request's glyph already says it opens; any other row with only a link gets the mark.
		const tail = pick?.url && n.kind !== "review" ? LINK : undefined;
		out.push(row(spread([{ text, fg: colour }], wait, width - 2, tail), pick, state, width));
	}
	out.push({ line: heading("UNDERWAY", width) });
	if (today.underway.length === 0) out.push({ line: quiet(["no thread at work", "n starts one"], width) });
	for (const u of today.underway) {
		// Kept in this order as the row narrows: the state last of all (0581, UX-4).
		const facts: Segment[] = [
			{ text: u.state ?? "working", fg: u.state === "idle" ? P.fg3 : P.green },
			{ text: u.since ? `  ${waitLabel(now.getTime() - u.since)}` : "", fg: P.fg2 },
			{ text: u.cost !== undefined ? `  ${money(u.cost)}` : "", fg: P.fg2 },
			{ text: u.context !== undefined ? `  ${tokens(u.context)}` : "", fg: P.purple },
		];
		const name = `${u.project ? `${u.project}/` : ""}${u.title}`;
		out.push(row(spread([{ text: name, fg: P.fg }], facts, width - 2), pickOf(u.paneId, undefined), state, width));
	}
	if (today.landed.length > 0) {
		out.push({ line: heading("LANDED TODAY", width) });
		for (const l of today.landed) {
			const text = `${l.project ? `${l.project}/` : ""}${l.title}${l.paneId ? "" : ` (${l.line})`}`;
			out.push(row([{ text, fg: l.paneId ? P.fg : P.fg2 }], pickOf(l.paneId, undefined), state, width));
		}
	}
	// Meetings still ahead, and a started one until its end, as "now" (0581, UX-14).
	const clock = hhmm(now);
	const meetings = today.calendar.filter(
		(c) => c.time === "all day" || c.time >= clock || (c.end !== undefined && c.end > clock),
	);
	out.push({ line: heading("CALENDAR", width) });
	if (meetings.length === 0) out.push({ line: quiet(["nothing more on the calendar today"], width) });
	for (const c of meetings) {
		const when = c.time === "all day" ? c.time : c.time < clock ? "now".padEnd(5) : c.time;
		const pick = pickOf(undefined, c.url);
		const line: Line = [
			{ text: `${when} `, fg: P.cyan },
			{ text: c.title, fg: P.fg },
		];
		out.push(row(spread(line, [], width - 2, pick ? LINK : undefined), pick, state, width));
	}
	return out;
}

/** His last key's outcome over up to two rows (0581, UX-5), gone a minute after it came. */
function said(state: TodayState, now: Date, width: number): Line[] {
	const outcome = state.said;
	if (!outcome || now.getTime() - outcome.at >= SAID_MS) return [];
	const fg = outcome.isError ? P.red : P.green;
	const lines = wrap(outcome.text, width);
	const shown = lines.length > 2 ? [lines[0] ?? "", cut(lines.slice(1).join(" "), width)] : lines;
	return shown.map((text) => fit([{ text, fg }], width));
}

/**
 * The pane's frame for `width` × `height`: the header, the sections (scrolled only as far as the picked row needs),
 * NEXT and NEWS in whatever rows are left, then his last key's outcome and the key line. Every line fits the width.
 */
export function render(state: TodayState, now: Date, width: number, height: number): Frame {
	const bottom = [...said(state, now, width), keyLine(width)];
	const top = state.up ? header(state, now, width) : [offline(state, now, width)];
	top.push([]);
	const room = Math.max(0, height - top.length - bottom.length);
	let rows = body(state, now, width);
	const picks = rows.flatMap((r) => (r.pick ? [r.pick] : []));
	if (rows.length > room) {
		const picked = rows.findIndex((r) => r.pick !== undefined && r.pick.key === state.selected);
		const start = picked < room ? 0 : picked - room + 1;
		rows = rows.slice(start, start + room);
	}
	const lines = [...top, ...rows.map((r) => r.line)];
	const spare = () => height - lines.length - bottom.length;
	// His notes after the calendar, while rows remain (0581, UX-14).
	const notes = state.today?.next ?? [];
	if (spare() >= 2 && notes.length > 0) {
		lines.push(heading("NEXT", width));
		for (const n of notes.slice(0, spare())) {
			const text = `${n.project ? `${n.project}: ` : ""}${n.text} (#${n.id})`;
			lines.push(fit([{ text: `  ${text}`, fg: P.fg }], width));
		}
	}
	const news = state.dashboard?.news.flatMap((feed) => feed.items.map((item) => ({ feed: feed.heading, item }))) ?? [];
	if (spare() >= 3 && news.length > 0) {
		lines.push([], heading("NEWS", width));
		for (const { feed, item } of news.slice(0, spare())) {
			const pick = pickOf(undefined, item.url);
			const line: Line = [
				{ text: `${feed}: `, fg: P.fg3 },
				{ text: item.title, fg: P.fg2 },
			];
			lines.push(row(spread(line, [], width - 2, pick ? LINK : undefined), pick, state, width).line);
			if (pick) picks.push(pick);
		}
	}
	while (lines.length < height - bottom.length) lines.push([]);
	const unique = [...new Map(picks.map((p) => [p.key, p])).values()];
	return { lines: [...lines.slice(0, Math.max(0, height - bottom.length)), ...bottom].slice(0, height), picks: unique };
}
