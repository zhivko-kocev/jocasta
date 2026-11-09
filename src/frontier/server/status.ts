/**
 * What the service shows of its threads (0402, 0403, 0409, 0538, 0540, 0546): herdr's sidebar tokens, the HUD,
 * `today`'s thread rows, `thread_status` and `projects`, each thread's ledger and task file, and their overlaps
 * (0456).
 */
import { statSync } from "node:fs";
import { join } from "node:path";
import { writeAtomically } from "../../shared/fsx.ts";
import { ledgerLine, type ThreadLedger, threadLedger } from "../../shared/ledger.ts";
import { log } from "../../shared/log.ts";
import { dataDir } from "../../shared/paths.ts";
import { listProjects, type Project } from "../../shared/projects.ts";
import { NOT_RUNNING } from "../../shared/service-file.ts";
import type { Hud, Leftover, Today } from "../protocol.ts";
import type { ServiceContext } from "./context.ts";
import type { DayThread } from "./day.ts";
import { type HerdrSnapshot, STOP_PING_MS } from "./herdr.ts";
import { reportLine } from "./report.ts";
import type { SessionStats } from "./sessions.ts";
import { spendToday } from "./spend.ts";
import { saveTask, taskFile } from "./task-file.ts";
import { subject, type Thread, threadState } from "./thread.ts";
import { money, overlapSaid, reviewSaid, shortModel } from "./wording.ts";
import { findLeftovers, gitOut } from "./worktrees.ts";

/** The sidebar tokens expire unless reported again within this time, so a stopped service leaves none (UX-13). */
const METADATA_TTL_MS = 15 * 60_000;
/** Unchanged tokens are reported again after this long, well inside their TTL. */
const METADATA_RENEW_MS = 5 * 60_000;
/** How long `stop` waits for herdr to take the cleared tokens. */
const CLEAR_WAIT_MS = 2_000;
/** How long a thread's changed files are kept before git is asked again (0456's overlaps). */
const OVERLAP_KEPT_MS = 60_000;
/** A thread's ledger is read again at most this often, and only when one of its session files grew (SVC-8). */
const LEDGER_KEPT_MS = 5_000;
/** The spend at which omp warns in a thread's pane, and the boss hears it once (0541, PRM-11). */
const SPEND_WARNED_USD = 50;

type Overlap = { title: string; files: string[] };

/**
 * A thread row's line in `today` (0409): its question or plan while it waits on him, its review while done, else the
 * last line its session said.
 */
function railLine(t: Thread, stats: SessionStats | undefined): string {
	if (t.shown === "blocked") return t.plan?.awaiting ? "plan ready" : (stats?.question ?? "awaiting you");
	if (t.shown !== "done") return stats?.lastLine ?? "";
	if (t.phase === "review") return "in review";
	const result = t.review?.result;
	if (result === undefined) return "done";
	return `done and reviewed: ${reviewSaid(result.notes, result.tests, result.blocking)}`;
}

export class StatusViews {
	/** herdr's workspaces as last read, for the project workspaces' tokens. */
	workspaces: HerdrSnapshot["workspaces"] = [];
	/** `jocasta setup`'s pending changes, said once as a notification (0466). */
	setupNotice: string | undefined;
	/** Sidebar tokens last reported, per pane and per project workspace, and when, so unchanged ones wait to renew. */
	private readonly reported = new Map<string, { json: string; at: number }>();
	private hudShown = "";
	/** The HUD's figures as last pushed to the page (FE-4): sent whenever they change, the quota's included. */
	private hudSent = "";
	/** Each thread's changed files, by pane, and when they were read: read again after a minute (0456). */
	private readonly changedFiles = new Map<string, { at: number; files: string[] }>();
	/** Each thread's changed files being read now, by pane, so one read serves every caller meanwhile. */
	private readonly filesReading = new Map<string, Promise<string[]>>();
	/** Each thread's ledger as last read (SVC-8, SRV-B-18): its current session, its files' sizes, when, and the ledger. */
	private readonly ledgers = new Map<
		Thread,
		{ at: number; path: string; sizes: string; ledger: ThreadLedger | undefined }
	>();

	constructor(private readonly c: ServiceContext) {}

	/** Forgets what was read of a closed or finished thread. */
	forget(thread: Thread): void {
		this.changedFiles.delete(thread.paneId);
		this.ledgers.delete(thread);
	}

	/**
	 * A thread's spend past $50 (0541, PRM-11): omp warned in its pane, and the boss hears it once. A thread found in
	 * herdr already past it was warned before this run.
	 */
	async spendPassed(): Promise<void> {
		if (!this.c.warmed) return;
		for (const thread of this.c.threads.all()) {
			const spend = this.ledgerOf(thread)?.spend;
			if (spend === undefined) continue;
			if (thread.spendSaid === undefined) thread.spendSaid = spend >= SPEND_WARNED_USD;
			if (thread.spendSaid || spend < SPEND_WARNED_USD) continue;
			thread.spendSaid = true;
			await this.c.events.thread(
				thread,
				`spend passed ${money(SPEND_WARNED_USD)} (${money(spend)}); omp warned in its pane`,
				"normal",
			);
		}
	}

	/**
	 * Sidebar tokens per thread pane and project workspace (0402, UX-04, UX-27), each expiring unless renewed (UX-13),
	 * then the HUD (0403).
	 */
	async report(): Promise<void> {
		const { herdr, sessions } = this.c;
		// A project's workspace is the first with its name (as `projectWorkspace` finds it): a thread titled like its
		// project has one of the same label, made later (SVC-14).
		const workspaceOf = new Map<string, string>();
		for (const w of this.workspaces) if (!workspaceOf.has(w.label)) workspaceOf.set(w.label, w.id);
		const counts = new Map<string, { working: number; awaiting: number }>();
		const live = new Set<string>();
		for (const thread of this.c.threads.values()) {
			const stats = this.c.warmed && thread.sessionPath ? sessions.read(thread.sessionPath) : undefined;
			// The thread's whole cost, every session of its task (0582, SRV-B-18), as task.md and thread_status say it.
			const spend = stats ? this.ledgerOf(thread)?.spend : undefined;
			const meta = {
				tokens: {
					model: stats?.model ? shortModel(stats.model) : null,
					cost: spend !== undefined ? money(spend) : null,
				},
				// "review ready" once hunk's notes are counted (0394); an idle pane with its plan in review waits on him.
				stateLabels: {
					blocked: stats?.question ?? "awaiting you",
					...(thread.plan?.awaiting ? { idle: "plan ready", done: "plan ready" } : {}),
					...(!thread.plan?.awaiting && thread.review?.result ? { done: "review ready" } : {}),
				},
				ttlMs: METADATA_TTL_MS,
			};
			live.add(`pane:${thread.paneId}`);
			await this.reportOnce(`pane:${thread.paneId}`, meta, () => herdr.reportPaneMetadata(thread.paneId, meta));
			const workspaceId = workspaceOf.get(thread.project.name);
			if (!workspaceId) continue;
			const count = counts.get(workspaceId) ?? { working: 0, awaiting: 0 };
			if (thread.state === "working") count.working++;
			if (thread.shown === "blocked") count.awaiting++;
			counts.set(workspaceId, count);
		}
		for (const key of [...this.reported.keys()]) {
			const workspaceId = key.startsWith("ws:") ? key.slice(3) : undefined;
			// A closed workspace's tokens went with it: never reported again (CO-13).
			if (workspaceId && !this.workspaces.some((w) => w.id === workspaceId)) this.reported.delete(key);
			else if (workspaceId && !counts.has(workspaceId)) counts.set(workspaceId, { working: 0, awaiting: 0 });
			else if (key.startsWith("pane:") && !live.has(key)) this.reported.delete(key);
		}
		for (const [workspaceId, { working, awaiting }] of counts) {
			const said = [working ? `${working} at work` : "", awaiting ? `${awaiting} awaiting you` : ""];
			const tokens = { running: said.filter(Boolean).join(" · ") || null };
			const key = `ws:${workspaceId}`;
			if (tokens.running === null) {
				// Cleared once, then forgotten: nothing left to renew (CO-13).
				if (!this.reported.has(key)) continue;
				try {
					await herdr.reportWorkspaceMetadata(workspaceId, tokens, METADATA_TTL_MS);
					this.reported.delete(key);
				} catch (error) {
					this.c.warn(error);
				}
				continue;
			}
			await this.reportOnce(key, tokens, () => herdr.reportWorkspaceMetadata(workspaceId, tokens, METADATA_TTL_MS));
		}
		if (this.setupNotice) {
			this.c.events.send({ type: "toast", reason: "event", level: "info", text: this.setupNotice });
			this.setupNotice = undefined;
		}
		const bossNotice = this.c.panes.bossNotice;
		if (bossNotice) {
			this.c.events.send({ type: "toast", reason: "event", level: "warning", text: bossNotice });
			this.c.panes.bossNotice = undefined;
		}
		// One `today` for the HUD and the page (CO-15).
		const today = this.c.day.today();
		this.writeHud(today);
		this.c.day.pushToday(today);
	}

	/** Sends a report when it changed, or when the last one is old enough to renew before herdr expires it. */
	private async reportOnce(key: string, value: unknown, send: () => Promise<void>): Promise<void> {
		const json = JSON.stringify(value);
		const last = this.reported.get(key);
		if (last?.json === json && Date.now() - last.at < METADATA_RENEW_MS) return;
		try {
			await send();
			this.reported.set(key, { json, at: Date.now() });
		} catch (error) {
			this.c.warn(error);
		}
	}

	/**
	 * The service stops (UX-13): the HUD says Jocasta is off, and the sidebar's tokens are cleared when herdr answers;
	 * tokens a herdr that doesn't answer keeps expire with their TTL.
	 */
	async clear(): Promise<void> {
		const herdr = this.c.herdr;
		try {
			writeAtomically(join(dataDir(), "hud.txt"), `${NOT_RUNNING}\n`);
		} catch (error) {
			log("warn", "service", `hud.txt wasn't written: ${(error as Error).message}`);
		}
		// Never through a stopped server: a request would start it again (0431), on the way out.
		const up = await Promise.race([
			herdr.ping().then(
				() => true,
				() => false,
			),
			Bun.sleep(STOP_PING_MS).then(() => false),
		]);
		if (!up) return;
		const clears = [...this.reported.keys()].map((key) => {
			const [kind, id = ""] = key.split(/:(.*)/s);
			return kind === "pane"
				? herdr.reportPaneMetadata(id, { tokens: { model: null, cost: null } })
				: herdr.reportWorkspaceMetadata(id, { running: null });
		});
		await Promise.race([Promise.allSettled(clears), Bun.sleep(CLEAR_WAIT_MS)]);
	}

	hud(today: Today = this.c.day.today()): Hud {
		const threads = this.c.threads.all();
		return {
			running: threads.filter((t) => t.state === "working").length,
			// Waiting on him: herdr's blocked, and a plan waiting in omp's Plan Review (herdr says idle there).
			blocked: threads.filter((t) => t.shown === "blocked").length,
			// Every row of Awaiting you, the count `today` and the HUD say (0432, 0581).
			awaiting: today.needsYou.length,
			// Counted once today's sessions were read; until then the HUD says "…".
			spendToday: this.c.warmed ? spendToday(this.c.sessions) : null,
			// The subscription windows from `omp usage` (0456), once it answered.
			quota: this.c.day.quota(),
			// today's header says quiet hours (0581).
			quiet: this.c.day.quiet(),
		};
	}

	/**
	 * `<data>/hud.txt` (0403, 0581), rewritten when it changes: NEEDS YOU's rows and today's spend beside herdr's clock,
	 * `7 awaiting you · $48.73 today`; the quota is in `today` (0554). The page gets the figures whenever any of them
	 * changed, the quota's included (FE-4), even when the write failed (then it is tried again on the next change or
	 * tick, CO-18).
	 */
	private writeHud(today: Today): void {
		const hud = this.hud(today);
		const spend = hud.spendToday === null ? "…" : money(hud.spendToday);
		const text = `${hud.awaiting} awaiting you · ${spend} today`;
		if (text !== this.hudShown) {
			try {
				writeAtomically(join(dataDir(), "hud.txt"), `${text}\n`);
				this.hudShown = text;
			} catch (error) {
				this.c.warn(new Error(`hud.txt wasn't written: ${(error as Error).message}`));
			}
		}
		const json = JSON.stringify(hud);
		if (json === this.hudSent) return;
		this.hudSent = json;
		this.c.events.send({ type: "hud", hud });
	}

	/**
	 * The threads closing Jocasta would cut off (0434, 0438), counted apart: awaiting him, the threads among the rows of
	 * Awaiting you the HUD and `today` count (0432: a question, a plan in omp's Plan Review, reviewed work), and working
	 * (`atWork`) among the rest.
	 */
	busyThreads(): { working: number; awaiting: number } {
		const awaited = this.awaitedPanes();
		let working = 0;
		for (const t of this.c.threads.values()) if (this.atWork(t, awaited)) working++;
		return { working, awaiting: [...this.c.threads.paneIds()].filter((paneId) => awaited.has(paneId)).length };
	}

	/** The panes of Awaiting you's rows (0432). */
	awaitedPanes(): Set<string> {
		return new Set(this.c.day.today().needsYou.flatMap((n) => (n.paneId ? [n.paneId] : [])));
	}

	/**
	 * A thread at work, as the close question counts it and Stop everything saves it for `--restore` (0434, 0549): a
	 * turn runs (herdr's or omp's working: a task, its review or its handoff) or one was started and not yet reported,
	 * and it isn't awaiting him.
	 */
	atWork(thread: Thread, awaited: Set<string>): boolean {
		return !awaited.has(thread.paneId) && (thread.state === "working" || thread.shown === "working");
	}

	/** A thread as `today` shows it (0409, 0555). */
	dayThread(t: Thread): DayThread {
		const stats = this.c.warmed && t.sessionPath ? this.c.sessions.read(t.sessionPath) : undefined;
		// The thread's whole cost (0582, SRV-B-18), as the sidebar says it.
		const spend = stats ? this.ledgerOf(t)?.spend : undefined;
		// The overlaps as last read; a read over a minute old is started again, never waited for (PERF-1).
		const overlaps = this.overlapsKnown(t).map((o) => o.title);
		return {
			paneId: t.paneId,
			title: t.title,
			project: t.project.name,
			line: railLine(t, stats),
			state: t.shown,
			changedAt: t.changedAt,
			...(t.review?.result?.at !== undefined ? { reviewedAt: t.review.result.at } : {}),
			...(overlaps.length > 0 ? { overlaps } : {}),
			...(spend !== undefined ? { cost: spend } : {}),
			...(stats?.context != null ? { context: stats.context } : {}),
		};
	}

	/**
	 * The files a thread changed since it left trunk, committed or not (`git diff --name-only <merge-base>` in its
	 * worktree), read off the event loop (PERF-1); one read at a time per thread.
	 */
	private readFiles(thread: Thread): Promise<string[]> {
		const running = this.filesReading.get(thread.paneId);
		if (running) return running;
		const read = (async () => {
			const trunk = await this.c.places.trunk(thread.project);
			const base = trunk ? await gitOut(thread.path, ["merge-base", trunk, "HEAD"]) : undefined;
			const diff = base ? await gitOut(thread.path, ["diff", "--name-only", base]) : undefined;
			const files = (diff ?? "").split("\n").filter(Boolean);
			this.changedFiles.set(thread.paneId, { at: Date.now(), files });
			return files;
		})().finally(() => this.filesReading.delete(thread.paneId));
		this.filesReading.set(thread.paneId, read);
		return read;
	}

	/**
	 * The overlaps `today` shows (0456): from each thread's files as last read; a thread's read over a minute old (or
	 * none yet) starts again in the background and shows on a later look.
	 */
	private overlapsKnown(thread: Thread): Overlap[] {
		const known = (t: Thread) => {
			const cached = this.changedFiles.get(t.paneId);
			if ((!cached || cached.at < Date.now() - OVERLAP_KEPT_MS) && !this.c.stopping) {
				const read = this.readFiles(t).catch((error) => this.c.warn(error));
				// The stop waits for it, like a call: its git runs in the thread's folder.
				this.c.calls.add(read);
				void read.finally(() => this.c.calls.delete(read));
			}
			return cached?.files ?? [];
		};
		return this.overlapsOf(thread, known);
	}

	/**
	 * The project's other open threads that touch the same files as `thread`, with those files (0456): every thread's
	 * files read now, all at once, and once per call (SVC-4): `files` holds the call's reads, shared by all its lines.
	 */
	async overlaps(thread: Thread, files: Map<Thread, Promise<string[]>> = new Map()): Promise<Overlap[]> {
		const read = (t: Thread) => {
			let found = files.get(t);
			if (!found) {
				found = this.readFiles(t);
				files.set(t, found);
			}
			return found;
		};
		const project = this.c.threads.ofProject(thread.project.id);
		const all = new Map(await Promise.all(project.map(async (t) => [t, await read(t)] as const)));
		return this.overlapsOf(thread, (t) => all.get(t) ?? []);
	}

	/** The overlaps of `thread` with its project's other threads, by each thread's `files`. */
	private overlapsOf(thread: Thread, files: (t: Thread) => string[]): Overlap[] {
		const mine = new Set(files(thread));
		if (mine.size === 0) return [];
		const found: Overlap[] = [];
		for (const other of this.c.threads.values()) {
			if (other === thread || other.project.id !== thread.project.id) continue;
			const shared = files(other).filter((file) => mine.has(file));
			if (shared.length > 0) found.push({ title: other.title, files: shared });
		}
		return found;
	}

	/**
	 * `thread_status` and `jocasta thread status|list`: each thread's line with its report's outcome once it handed one
	 * in for its current task (0539), and under it its ledger line (0538, 0546) once its session has model calls; a
	 * named thread also shows its task file and the whole report. Each thread's files are read once for the whole
	 * answer, all at once (SVC-4).
	 */
	async threadStatus(wanted: string | undefined, project?: Project): Promise<string> {
		const files = new Map<Thread, Promise<string[]>>();
		const reportIn = (t: Thread) => this.c.review.reportIn(t);
		if (wanted) {
			const thread = this.c.threads.find(wanted);
			const report = thread.task?.report;
			const spent = this.ledgerSaid(thread);
			return [
				await this.threadLine(thread, files),
				...(spent ? [spent] : []),
				...(thread.task ? [`task file: ${taskFile(thread.task)}`] : []),
				`report: ${report ? `${reportLine(report.value)}${reportIn(thread) ? "" : " (an earlier task's)"}` : "none yet"}`,
			].join("\n");
		}
		const threads = this.c.threads.all().filter((t) => !project || t.project.id === project.id);
		if (threads.length === 0) return project ? `No threads in ${project.name}.` : "No threads.";
		const lines = await Promise.all(
			threads.map(async (t) => {
				const report = t.task?.report && reportIn(t) ? ` · reported ${t.task.report.value.outcome}` : "";
				const spent = this.ledgerSaid(t);
				return `${await this.threadLine(t, files)}${report}${spent ? `\n  ${spent}` : ""}`;
			}),
		);
		return lines.join("\n");
	}

	/** A thread's ledger line (0538, 0546), read as its sessions are now; undefined before its first model call. */
	private ledgerSaid(thread: Thread): string | undefined {
		const ledger = this.ledgerOf(thread, { fresh: true });
		return ledger && ledger.requests > 0 ? ledgerLine(ledger) : undefined;
	}

	/**
	 * A thread's ledger (0538, 0546) over every omp session of it: the ones in its worktree's session folder that began
	 * since its task did ("Approve and execute" starts a new one), or its current one alone without a task file. Parsed
	 * again only when one of those files changed size (SVC-8). The refresh tick's callers look at most every few
	 * seconds; `fresh` (a tool's answer, the task file) and a new current session always look.
	 */
	ledgerOf(thread: Thread, { fresh = false }: { fresh?: boolean } = {}): ThreadLedger | undefined {
		if (!thread.sessionPath) return undefined;
		const kept = this.ledgers.get(thread);
		const now = Date.now();
		if (kept && !fresh && kept.path === thread.sessionPath && now - kept.at < LEDGER_KEPT_MS) return kept.ledger;
		try {
			const sessions = this.c.sessions;
			const current = sessions.moved(thread.sessionPath);
			const files = thread.task ? sessions.sessionsSince(current, thread.task.createdAt) : [current];
			const sizes = files
				.map((file) => {
					try {
						return `${file}|${statSync(file).size}`;
					} catch {
						return `${file}|gone`;
					}
				})
				.join("\n");
			if (kept && kept.sizes === sizes) {
				kept.at = now;
				kept.path = thread.sessionPath;
				return kept.ledger;
			}
			const ledger = threadLedger(files);
			this.ledgers.set(thread, { at: now, path: thread.sessionPath, sizes, ledger });
			return ledger;
		} catch (error) {
			this.c.warn(new Error(`${subject(thread)}'s ledger wasn't read: ${(error as Error).message}`));
			return undefined;
		}
	}

	/** `projects`: each project with its threads' lines and its leftover worktrees (0456). */
	async projectsText(): Promise<string> {
		await this.c.places.sync();
		const projects = listProjects(this.c.catalog);
		if (projects.length === 0) return "No projects: add folders to the watched roots in projects.yaml.";
		const leftovers = await this.leftovers();
		const files = new Map<Thread, Promise<string[]>>();
		const blocks = await Promise.all(
			projects.map(async (p) => {
				const threads = this.c.threads.ofProject(p.id);
				const lines = [`${p.name} (${p.id})${p.git ? "" : ", not git"}: ${p.mainPath}`];
				for (const line of await Promise.all(threads.map((t) => this.threadLine(t, files)))) lines.push(`  ${line}`);
				for (const l of leftovers.filter((l) => l.project === p.name)) {
					lines.push(
						`  left on disk: ${l.folder} [${l.branch ?? "detached"}] ${l.ahead} ahead, ${l.dirty === 0 ? "clean" : `${l.dirty} changed`}`,
					);
				}
				return lines.join("\n");
			}),
		);
		return blocks.join("\n");
	}

	/** A thread's line in `thread_status` and `projects`; `files` is the answer's one read of each thread's files. */
	private async threadLine(t: Thread, files: Map<Thread, Promise<string[]>>): Promise<string> {
		const day = this.dayThread(t);
		const state = threadState(t);
		const line = day.line && !t.plan?.awaiting && day.line !== state ? ` — ${day.line}` : "";
		const overlaps = overlapSaid(await this.overlaps(t, files));
		return `${subject(t)} [${t.branch}] ${state}${line}${overlaps ? ` · overlaps ${overlaps}` : ""}`;
	}

	/**
	 * Leftover worktrees (0456): checkouts in a project's `<project>.wt/` folder that no open thread holds, read off the
	 * event loop. Never removed here.
	 */
	leftovers(): Promise<Leftover[]> {
		return findLeftovers(
			this.c.places.listed(),
			this.c.threads.all().map((t) => t.path),
			(project) => this.c.places.trunk(project),
		);
	}

	/**
	 * Brings the thread's task file up to date (0540): its session, state and spend (its ledger's, over all its
	 * sessions, 0538) with whatever else changed on the record; written only when the text changed. A failed write is
	 * logged, never the thread's problem.
	 */
	writeTask(thread: Thread): void {
		const task = thread.task;
		if (!task || !this.c.threads.current(thread)) return;
		task.session = thread.sessionPath ?? task.session;
		const ledger = this.c.warmed ? this.ledgerOf(thread, { fresh: true }) : undefined;
		task.spend = ledger ? ledger.spend : task.spend;
		const state = threadState(thread);
		task.state = state === "done" && thread.review?.result?.at !== undefined ? "done and reviewed" : state;
		try {
			saveTask(task);
		} catch (error) {
			this.c.warn(new Error(`${subject(thread)}'s task file wasn't written: ${(error as Error).message}`));
		}
	}
}
