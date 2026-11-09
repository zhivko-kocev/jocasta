import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { dayLabel, dayStart, localDay } from "../../shared/day.ts";
import { writeAtomically } from "../../shared/fsx.ts";
import { gitAsync } from "../../shared/git.ts";
import { dataDir } from "../../shared/paths.ts";
import type { Project } from "../../shared/projects.ts";

/** One omp session of a project's for the worklog: its label (title, else checkout) and its messages' times. */
export interface WorklogSession {
	label: string;
	times: readonly number[];
}

/**
 * A project's omp sessions (in its main checkout and its worktrees) that may hold messages of `day`; the service
 * reads them from omp's files.
 */
type SessionsOf = (project: Project, day: string) => WorklogSession[];

/** Gaps between a thread's messages up to this long count as time spent; longer ones are breaks. */
const ACTIVE_GAP_MS = 15 * 60_000;
/** A worklog's `git log` (and `git config`) of one project (CO-12): off the event loop, bounded. */
const GIT_TIMEOUT_MS = 60_000;
/** Projects whose git runs at once while a worklog is written (PERF-2). */
const GIT_POOL = 4;

function worklogDir(): string {
	return join(dataDir(), "worklog");
}

export function worklogPath(day: string): string {
	return join(worklogDir(), `${day}.md`);
}

/** Time spent: the sum of gaps between messages up to 15 minutes apart. */
function activeMs(times: readonly number[]): number {
	let total = 0;
	for (let i = 1; i < times.length; i++) {
		const gap = (times[i] as number) - (times[i - 1] as number);
		if (gap <= ACTIVE_GAP_MS) total += gap;
	}
	return total;
}

function duration(ms: number): string {
	const minutes = Math.round(ms / 60_000);
	if (minutes < 60) return `${minutes}m`;
	return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
}

/** A commit's web page from the checkout's `origin`: GitHub or Azure DevOps; undefined for other hosts. */
function commitUrl(origin: string | undefined, sha: string): string | undefined {
	if (!origin) return undefined;
	const github = /github\.com[:/]([^/]+\/[^/]+?)(?:\.git)?\/?$/.exec(origin);
	if (github) return `https://github.com/${github[1]}/commit/${sha}`;
	const azure = /dev\.azure\.com\/([^/]+)\/([^/]+)\/_git\/([^/]+?)\/?$/.exec(
		origin.replace(/^https:\/\/[^@/]+@/, "https://"),
	);
	if (azure) return `https://dev.azure.com/${azure[1]}/${azure[2]}/_git/${azure[3]}/commit/${sha}`;
	const ssh = /ssh\.dev\.azure\.com:v3\/([^/]+)\/([^/]+)\/([^/]+?)\/?$/.exec(origin);
	if (ssh) return `https://dev.azure.com/${ssh[1]}/${ssh[2]}/_git/${ssh[3]}/commit/${sha}`;
	return undefined;
}

type Commit = { sha: string; subject: string; url?: string };

/** What the worklog needs to know about a git project: his `user.email` there and its `origin`. */
export interface GitPlace {
	email?: string;
	origin?: string;
}

/** A git project's `user.email` and `origin` (two git calls off the event loop); empty for a project outside git. */
async function gitPlace(project: Project): Promise<GitPlace> {
	if (!project.git) return {};
	const value = async (args: string[]) => {
		const { code, stdout } = await gitAsync(project.mainPath, args, GIT_TIMEOUT_MS);
		return code === 0 ? stdout.trim() || undefined : undefined;
	};
	const [email, origin] = await Promise.all([value(["config", "user.email"]), value(["remote", "get-url", "origin"])]);
	return { ...(email ? { email } : {}), ...(origin ? { origin } : {}) };
}

/** Each project's git email and origin, kept for one local day (CO-12): both the worklog and its so-far pass ask. */
export class GitPlaces {
	private day = "";
	private readonly byProject = new Map<string, Promise<GitPlace>>();

	of(project: Project, day: string): Promise<GitPlace> {
		if (this.day !== day) {
			this.day = day;
			this.byProject.clear();
		}
		let place = this.byProject.get(project.id);
		if (!place) {
			place = gitPlace(project);
			this.byProject.set(project.id, place);
		}
		return place;
	}
}

/** `git log` of his commits on `day`, on any branch. */
function logArgs(day: string, email: string): string[] {
	return [
		"log",
		"--all",
		`--since=${new Date(dayStart(day)).toISOString()}`,
		`--until=${new Date(dayStart(day, 1)).toISOString()}`,
		`--author=${email}`,
		"--format=%H%x09%s",
	];
}

function parseCommits(out: string, origin: string | undefined): Commit[] {
	return out
		.split("\n")
		.filter(Boolean)
		.map((line) => {
			const [sha = "", ...subject] = line.split("\t");
			const url = commitUrl(origin, sha);
			return { sha, subject: subject.join("\t"), ...(url ? { url } : {}) };
		});
}

/**
 * His commits on `day` in a git project, on any branch, without holding the event loop (CO-12, PERF-2): author email
 * from the checkout's `user.email` (`place`, from the caller's per-day cache); a `git log` that fails or runs past a
 * minute counts as none.
 */
export async function commitsAsync(project: Project, day: string, place: GitPlace): Promise<Commit[]> {
	if (!project.git || !place.email) return [];
	const { code, stdout } = await gitAsync(project.mainPath, logArgs(day, place.email), GIT_TIMEOUT_MS);
	return code === 0 ? parseCommits(stdout, place.origin) : [];
}

/** A project's day (0302): time per thread from session activity, and his commits. */
interface ProjectDay {
	threads: { label: string; ms: number }[];
	commits: Commit[];
	totalMs: number;
}

/** A day's work in one project, or undefined when nothing happened in it; `mine` are its commits on that day. */
export function projectDay(day: string, sessions: readonly WorklogSession[], mine: Commit[]): ProjectDay | undefined {
	const threads = sessions
		.map((s) => {
			const times = s.times.filter((at) => localDay(at) === day).sort((a, b) => a - b);
			return { label: s.label, times, ms: activeMs(times) };
		})
		.filter((t) => t.times.length > 0)
		.map(({ label, ms }) => ({ label, ms }));
	if (threads.length === 0 && mine.length === 0) return undefined;
	return { threads, commits: mine, totalMs: threads.reduce((sum, t) => sum + t.ms, 0) };
}

/** A project's day as one line for the dashboard (0429): `acme-api · 1h 20m · 3 commits`. */
export function projectDayLine(name: string, day: ProjectDay): string {
	const n = day.commits.length;
	return [name, duration(day.totalMs), ...(n > 0 ? [`${n} commit${n === 1 ? "" : "s"}`] : [])].join(" · ");
}

/** A project's generated entry for `day`, or undefined when nothing happened in it. */
function projectEntry(
	project: Project,
	day: string,
	sessions: readonly WorklogSession[],
	mine: Commit[],
): string | undefined {
	const found = projectDay(day, sessions, mine);
	if (!found) return undefined;
	const { threads, totalMs } = found;
	const lines = [`## ${project.name} · ${duration(totalMs)}`, ""];
	if (threads.length > 0) {
		lines.push("Threads");
		for (const { label, ms } of threads) lines.push(`- ${label} · ${duration(ms)}`);
	}
	if (mine.length > 0) {
		if (threads.length > 0) lines.push("");
		lines.push("Commits");
		for (const c of mine) {
			const short = c.sha.slice(0, 7);
			lines.push(`- ${c.url ? `[${short}](${c.url})` : short} ${c.subject}`);
		}
	}
	return lines.join("\n");
}

const hash = (text: string) => createHash("sha256").update(text).digest("hex").slice(0, 12);
const BLOCK = /<!-- jocasta (\S+) (\S+) -->\n([\s\S]*?)\n<!-- \/jocasta \1 -->/g;

function block(projectId: string, body: string): string {
	return `<!-- jocasta ${projectId} ${hash(body)} -->\n${body}\n<!-- /jocasta ${projectId} -->`;
}

/**
 * Merges new entries into a worklog he may have edited (0302): each project's entry sits between markers that carry
 * the hash of what Jocasta wrote. A block whose text still matches its hash is replaced; a block he edited is kept
 * as he left it; projects without a block are appended. Text outside the blocks is his and stays.
 */
export function mergeWorklog(
	existing: string | undefined,
	heading: string,
	entries: ReadonlyMap<string, string>,
): string {
	if (existing === undefined) {
		return `${[heading, ...[...entries].map(([id, body]) => block(id, body))].join("\n\n")}\n`;
	}
	const seen = new Set<string>();
	const merged = existing.replace(BLOCK, (whole, id: string, recorded: string, body: string) => {
		seen.add(id);
		const next = entries.get(id);
		if (next === undefined || hash(body) !== recorded) return whole;
		return block(id, next);
	});
	const added = [...entries].filter(([id]) => !seen.has(id)).map(([id, body]) => block(id, body));
	return added.length === 0 ? merged : `${merged.trimEnd()}\n\n${added.join("\n\n")}\n`;
}

/**
 * Writes `<data>/worklog/<day>.md` (0302): one entry per project from that day's omp sessions and his commits, with
 * commit links and time per project from session activity. Each project's git runs off the event loop, four
 * projects at a time (PERF-2); once `stopped` says so (the service stopping meanwhile) nothing is written. Returns the
 * path and the projects it covers.
 */
export async function writeWorklog(
	projects: readonly Project[],
	day: string,
	sessionsOf: SessionsOf,
	places: GitPlaces = new GitPlaces(),
	stopped: () => boolean = () => false,
): Promise<{ path: string; projects: string[] }> {
	const found = new Map<string, Commit[]>();
	let next = 0;
	const worker = async () => {
		while (next < projects.length && !stopped()) {
			const project = projects[next++] as Project;
			found.set(project.id, await commitsAsync(project, day, await places.of(project, day)));
		}
	};
	await Promise.all(Array.from({ length: Math.min(GIT_POOL, projects.length) }, worker));
	const path = worklogPath(day);
	if (stopped()) return { path, projects: [] };
	const entries = new Map<string, string>();
	for (const project of projects) {
		const entry = projectEntry(project, day, sessionsOf(project, day), found.get(project.id) ?? []);
		if (entry) entries.set(project.id, entry);
	}
	const existing = existsSync(path) ? readFileSync(path, "utf8") : undefined;
	const text = mergeWorklog(existing, `# Worklog · ${dayLabel(day, { year: true })}`, entries);
	if (text !== existing) writeAtomically(path, text);
	return { path, projects: projects.filter((p) => entries.has(p.id)).map((p) => p.name) };
}
