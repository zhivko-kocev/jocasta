import { randomBytes } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { hhmm } from "../../shared/day.ts";
import { readJsonObject, writeAtomically } from "../../shared/fsx.ts";
import { pathKey } from "../../shared/pathkey.ts";
import { projectDir } from "../../shared/paths.ts";
import { withoutLeadingComment } from "../../shared/prompt.ts";
import type { ReviewProof } from "./hunk.ts";
import { reportLine, type ThreadReport } from "./report.ts";
import type { TestsResult } from "./sessions.ts";
import { WHEN_YOU_FINISH } from "./wording.ts";

/**
 * A thread's task file (0540, K2): `<data>/projects/<pid>/threads/<tid>/task.md`, written by Jocasta only and kept
 * current as the thread moves, rendered from the thread's record `thread.json` beside it (which also holds its
 * `report_result`, 0539). `prompt.md` beside them is what each omp launch of the thread appends to its system prompt
 * (`--append-system-prompt`): the panes' shipped prompt (Ponytail, 0427), then the file's Brief and Done-when sections.
 */

/** One check of a review (0453, 0540): the command and its exit code in the review turn; null when it didn't run. */
export interface CheckExit {
	command: string;
	exit: number | null;
}

/**
 * One review as the task file lists it, with the outcome a restart gives the thread back (its notes, blocking notes,
 * tests and diff proof; absent in records written before they were kept).
 */
export interface TaskReview {
	at: number;
	passed: boolean;
	/** The project's check (when known) and each `done_when` command, in that order. */
	checks: CheckExit[];
	/** What the review found: `2 notes (1 blocking), tests pass`, and `no report` when the thread handed in none. */
	said: string;
	notes?: number;
	blocking?: number;
	tests?: TestsResult;
	proof?: ReviewProof;
}

/** `thread.json`: what the task file is rendered from. */
export interface TaskRecord {
	/** `t-` and 4 hex digits, unique in the project. */
	id: string;
	projectId: string;
	project: string;
	title: string;
	/** The worktree; empty until it is made. */
	path: string;
	branch: string;
	/** Trunk's commit the thread started from, short. */
	base: string | null;
	/** The thread's omp session file, once known. */
	session: string | null;
	/** The thread this one continues; no lineage yet. */
	parent: string | null;
	/** The boss's brief, or his own from New thread or `jocasta thread new`; empty when not recorded. */
	brief: string;
	/** Commands that must pass (0540), run by the review beside the project's check. */
	doneWhen: string[];
	createdAt: number;
	/** The state as `thread_status` says it. */
	state: string;
	/** USD, from its session; null before one is read. */
	spend: number | null;
	plan: { approvedAt: number; title: string; file: string | null } | null;
	report: { at: number; value: ThreadReport } | null;
	reviews: TaskReview[];
	/** The thread was finished or removed: its worktree folder may hold a new thread. */
	closed: boolean;
}

/** `<data>/projects/<pid>/threads`. */
export function threadsDir(projectId: string): string {
	return join(projectDir(projectId), "threads");
}

/** The record's folder. */
export function taskDir(record: Pick<TaskRecord, "projectId" | "id">): string {
	return join(threadsDir(record.projectId), record.id);
}

/** The task file's path. */
export function taskFile(record: Pick<TaskRecord, "projectId" | "id">): string {
	return join(taskDir(record), "task.md");
}

/** A new record with a fresh id; nothing is written until `saveTask`. */
export function newTask(
	fields: Pick<TaskRecord, "projectId" | "project" | "title" | "brief" | "doneWhen"> & Partial<TaskRecord>,
	now = Date.now(),
): TaskRecord {
	let id: string;
	do id = `t-${randomBytes(2).toString("hex")}`;
	while (existsSync(join(threadsDir(fields.projectId), id)));
	return {
		path: "",
		branch: "",
		base: null,
		session: null,
		parent: null,
		createdAt: now,
		state: "starting",
		spend: null,
		plan: null,
		report: null,
		reviews: [],
		closed: false,
		...fields,
		id,
	};
}

/** The open record of the thread in worktree `path`, the newest when several claim it; undefined when none. */
export function findTask(projectId: string, path: string): TaskRecord | undefined {
	const dir = threadsDir(projectId);
	if (!existsSync(dir)) return undefined;
	const key = pathKey(path);
	let found: TaskRecord | undefined;
	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		if (!entry.isDirectory()) continue;
		const record = readJsonObject(join(dir, entry.name, "thread.json")) as TaskRecord | undefined;
		if (!record || record.closed || !record.path || pathKey(record.path) !== key) continue;
		if (!found || record.createdAt > found.createdAt) found = record;
	}
	return found;
}

/** The Brief section, as the task file and each launch's appended prompt have it. */
function briefSection(record: Pick<TaskRecord, "brief">): string[] {
	return ["## Brief", record.brief.trim() || "Not recorded."];
}

/** The Done-when section (0540). */
function doneWhenSection(record: Pick<TaskRecord, "doneWhen">): string[] {
	return [
		"## Done when",
		...(record.doneWhen.length > 0
			? record.doneWhen.map((command) => `- \`${command}\` passes`)
			: ["- No commands given; the review's project check decides."]),
	];
}

/** The Brief and Done-when sections. */
function briefSections(record: Pick<TaskRecord, "brief" | "doneWhen">): string[] {
	return [...briefSection(record), "", ...doneWhenSection(record)];
}

/**
 * The approved plan's text from its file, or its path when the file can't be read. omp names it `local://<slug>-plan.md`,
 * which lives in the session's artifacts dir (`<session stem>/local/`).
 */
function planText(file: string | null, session: string | null): string {
	if (!file) return "The plan's file isn't recorded.";
	const local = /^local:\/\/(.+)$/.exec(file)?.[1];
	const path = local ? (session ? join(session.replace(/\.jsonl$/, ""), "local", local) : undefined) : file;
	try {
		if (path) return readFileSync(path, "utf8").trim();
	} catch {
		// Unreadable: named below.
	}
	return `In ${file}.`;
}

/** The task file's text (0540's format). */
export function taskMarkdown(record: TaskRecord): string {
	const session = record.session ? basename(record.session).replace(/\.jsonl$/, "") : "—";
	const lines = [
		`# ${record.title} (${record.project})`,
		"",
		`- Thread: ${record.id} · session ${session} · parent: ${record.parent ?? "—"} · base ${record.base ?? "—"}`,
		`- State: ${record.state} · spend ${record.spend === null ? "—" : `$${record.spend.toFixed(2)}`}`,
		"",
		...briefSections(record),
		"",
		...(record.plan
			? [`## Plan (approved ${hhmm(record.plan.approvedAt)})`, planText(record.plan.file, record.session)]
			: ["## Plan", "Not approved yet."]),
		"",
		"## Report",
		record.report ? reportLine(record.report.value) : "None yet.",
		"",
		"## Reviews",
		...(record.reviews.length > 0
			? record.reviews.map((review) =>
					[
						`- ${hhmm(review.at)} ${review.passed ? "pass" : "not a pass"}`,
						...review.checks.map((c) => `\`${c.command}\` ${c.exit === null ? "not run" : `exit ${c.exit}`}`),
						`notes: ${review.said}`,
					].join(" · "),
				)
			: ["None yet."]),
	];
	return `${lines.join("\n")}\n`;
}

/** Writes `thread.json` and `task.md` when they changed. */
export function saveTask(record: TaskRecord): void {
	const dir = taskDir(record);
	writeIfChanged(join(dir, "thread.json"), `${JSON.stringify(record, null, "\t")}\n`);
	writeIfChanged(join(dir, "task.md"), taskMarkdown(record));
}

function writeIfChanged(file: string, text: string): void {
	try {
		if (readFileSync(file, "utf8") === text) return;
	} catch {
		// Not there yet.
	}
	writeAtomically(file, text);
}

/** Marks the thread finished or removed, so its folder may hold a new thread; a failed write goes to `failed`. */
export function closeTask(record: TaskRecord, state: string, failed: (error: unknown) => void): void {
	record.closed = true;
	record.state = state;
	try {
		saveTask(record);
	} catch (error) {
		failed(error);
	}
}
/**
 * The text a thread's omp appends to its system prompt (0540, 0577): the panes' shipped prompt when there is one,
 * without its leading `<!-- … -->` comment (its license and file notes, PRM-6), then the Brief, Done-when and When you
 * finish sections. With the layer off (`layer` false, 0576's test switch) only the Brief follows the title: Done when
 * and When you finish are the layer's parts. The benchmark's A6 layer appends the same text (0543).
 */
export function taskPrompt(
	record: Pick<TaskRecord, "title" | "project" | "brief" | "doneWhen">,
	panePrompt: string | null,
	layer = true,
): string {
	let shipped = "";
	if (panePrompt) {
		try {
			shipped = withoutLeadingComment(readFileSync(panePrompt, "utf8")).trim();
		} catch {
			// Missing: the task's sections alone.
		}
	}
	const sections = layer ? [...briefSections(record), "", ...WHEN_YOU_FINISH] : briefSection(record);
	const task = [`# Your task: ${record.title} (${record.project})`, "", ...sections].join("\n");
	return `${shipped ? `${shipped}\n\n` : ""}${task}\n`;
}

/**
 * `prompt.md`, what this launch of the thread's omp appends to its system prompt (0540), {@link taskPrompt}'s text.
 * omp 18.18 takes one `--append-system-prompt`, the last one given (text, or a file's contents when the value names a
 * file), so the two go in one file. Resolves its path.
 */
export function launchPrompt(record: TaskRecord, panePrompt: string | null, layer = true): string {
	const file = join(taskDir(record), "prompt.md");
	writeIfChanged(file, taskPrompt(record, panePrompt, layer));
	return file;
}
