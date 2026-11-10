/**
 * The omps of the thread panes Jocasta started (0490, 0547, 0549, 0561): remembered by pid and start time, ended with
 * what they left when their pane goes, asked for their state through omp's host registry, and ended for `--restore`
 * when Jocasta stops everything.
 */
import { log } from "../../shared/log.ts";
import { sameDir } from "../../shared/pathkey.ts";
import { waitFor } from "../../shared/poll.ts";
import {
	endOmpLeftovers,
	endProcesses,
	mergeOmp,
	type OmpProcesses,
	ompProcesses,
	type ProcessId,
	processIdOf,
} from "../../shared/proctree.ts";
import type { ServiceContext } from "./context.ts";
import { type AgentState, type PaneInfo, STOP_PING_MS } from "./herdr.ts";
import { type HostState, readHosts, removeHostEntries } from "./hosts.ts";
import { subject, type Thread } from "./thread.ts";

/** How long the end of a thread pane's omp is waited for before its leftovers end (0490). */
export const LEFTOVER_WAIT_MS = 10_000;
/** When leftovers are looked for again, for workers an exiting omp started (0490). */
export const LEFTOVER_AGAIN_MS = 3_000;
/** How long Stop everything waits for the omps it ended to be gone before their host entries go (0547). */
const HOST_END_WAIT_MS = 5_000;
/** How often Stop everything looks whether the omps it ended are gone. */
const HOST_END_POLL_MS = 100;

/** omp's own state of a host (0547): a dialog waiting, else a turn running, else idle; unknown when it doesn't say. */
function hostState(host: HostState): AgentState | undefined {
	if (host.inputRequired) return "blocked";
	if (host.busy === null) return undefined;
	return host.busy ? "working" : "idle";
}

export class OmpTracker {
	/** Old omps whose leftovers are being ended (0490): Stop everything ends them too until their last pass ran. */
	private readonly endingOmps = new Set<OmpProcesses>();
	/** Threads Stop everything saves for `--restore` (0549): their omp ends without ending its turn first. */
	private readonly paused = new Set<Thread>();
	/** The omps of the panes Jocasta started, read as Stop everything begins: their host entries go once they ended. */
	private closingOmps: ProcessId[] = [];
	/** The host registry read under way (`poll`). */
	private hostPoll: Promise<void> | undefined;
	private readonly leftoverWaitMs: number;
	private readonly leftoverAgainMs: number;

	constructor(private readonly c: ServiceContext) {
		this.leftoverWaitMs = c.options.leftoverWaitMs ?? LEFTOVER_WAIT_MS;
		this.leftoverAgainMs = c.options.leftoverAgainMs ?? LEFTOVER_AGAIN_MS;
	}

	/**
	 * Remembers the omp of a thread pane Jocasta started (0490): the process herdr started for it and what runs under it,
	 * each by pid and start time, read for the pane's terminal `terminal`.
	 */
	async remember(thread: Thread, terminal?: string): Promise<void> {
		const paneId = thread.paneId;
		if (!this.c.launched.has(paneId)) return;
		let pid: number | undefined;
		try {
			pid = await this.c.herdr.paneRoot(paneId);
		} catch (error) {
			this.c.warn(error);
			return;
		}
		if (thread.paneId !== paneId) return;
		const shim = pid === undefined ? undefined : processIdOf(pid);
		thread.omp = { terminal, shim, processes: shim ? ompProcesses([shim])[0] : undefined };
	}

	/**
	 * Threads whose remembered omp is still only the process herdr started (the real omp, its child, not up yet when
	 * read): what runs under it now joins what is remembered, from one snapshot for all of them (0490).
	 */
	complete(byId: Map<string, PaneInfo>): void {
		const waiting = this.c.threads.all().filter((thread) => {
			const omp = thread.omp;
			const pane = byId.get(thread.paneId);
			return pane && omp?.shim && omp.terminal === pane.terminalId && (omp.processes?.omp.length ?? 0) < 2;
		});
		if (waiting.length === 0) return;
		const reads = ompProcesses(waiting.map((thread) => thread.omp?.shim as ProcessId));
		waiting.forEach((thread, i) => {
			const omp = thread.omp;
			const read = reads[i];
			if (!omp || !read) return;
			omp.processes = omp.processes ? mergeOmp(omp.processes, read) : read;
			// The shim ended without a child: nothing more to learn.
			if (!read.omp.some((id) => id.pid === omp.shim?.pid)) omp.shim = undefined;
		});
	}

	/**
	 * Ends what an old omp left (0490): waits up to `leftoverWaitMs` for it to end, ends its leftovers, then looks again
	 * `leftoverAgainMs` later for the workers it started on its way out. Resolves after the first pass.
	 */
	async end(processes: OmpProcesses): Promise<void> {
		this.endingOmps.add(processes);
		try {
			await endOmpLeftovers([processes], this.leftoverWaitMs);
		} catch (error) {
			this.c.warn(error);
		}
		void Bun.sleep(this.leftoverAgainMs)
			.then(() => endOmpLeftovers([processes], 0))
			.catch((error) => this.c.warn(error))
			.finally(() => this.endingOmps.delete(processes));
	}

	/**
	 * Stop everything and Keep's later stop, once herdr's server is gone (0486, 0490): every thread pane's remembered
	 * omp, and old ones still being ended, end with what they left, looked for twice as for a closed pane.
	 */
	async endLeftovers(): Promise<void> {
		const all = [
			...this.c.threads.all().flatMap((thread) => (thread.omp?.processes ? [thread.omp.processes] : [])),
			...this.endingOmps,
		];
		if (all.length === 0) return;
		await endOmpLeftovers(all, this.leftoverWaitMs);
		await Bun.sleep(this.leftoverAgainMs);
		await endOmpLeftovers(all, 0);
	}

	/**
	 * Before Jocasta stops everything (0549): each thread at work, as the close question counts it (`atWork`), keeps its
	 * session in `panes.json`, so the next start brings its omp back with `--restore` and its turn goes on; the others
	 * reopen idle with `--continue`. The omps of the panes Jocasta started are read while herdr still answers, so their
	 * host entries can go once they ended (`removeEndedHosts`).
	 */
	async pause(): Promise<void> {
		const launched = this.c.launched;
		const awaited = this.c.status.awaitedPanes();
		for (const thread of this.c.threads.values()) {
			if (!this.c.status.atWork(thread, awaited) || !thread.sessionPath) continue;
			launched.pause(thread.paneId, thread.sessionPath);
			this.paused.add(thread);
		}
		// Read at once, each bounded: a herdr that stopped answering doesn't hold Stop everything (SRV-A-3).
		const reads = await Promise.allSettled(
			launched.panes().map(async (paneId) => {
				const pid = await this.c.herdr.paneRoot(paneId, STOP_PING_MS);
				return pid === undefined ? undefined : processIdOf(pid);
			}),
		);
		const roots: ProcessId[] = [];
		reads.forEach((read, i) => {
			if (read.status === "fulfilled") {
				if (read.value) roots.push(read.value);
			} else {
				const paneId = launched.panes()[i];
				log("warn", "shutdown", `pane ${paneId}'s omp wasn't read: ${(read.reason as Error).message}`);
			}
		});
		this.closingOmps = roots.length > 0 ? ompProcesses(roots).flatMap((read) => read.omp) : [];
	}

	/**
	 * Stop everything, once the service stopped and before herdr's server does (0549): the omp of each thread saved for
	 * `--restore` ends at once with what runs under it (found by start-checked parent links, SRV-A-1), forced, so it
	 * records no end of its turn. omp's own teardown on a signal or a closing pane aborts the turn and records the exit
	 * (omp 18.18.6 `interactive-mode.ts:2467-2497`, `agent-session.ts:3939-3965`), after which `--restore` finds
	 * nothing unfinished (`exit-diagnostics.ts:151-157`); a process that just ends leaves the turn open, and `--restore`
	 * goes on with it (`agent-session.ts:13379-13409`). The other panes' omps end with herdr as before.
	 */
	async cutPaused(): Promise<void> {
		for (const thread of this.paused) {
			const roots = thread.omp?.processes?.tree ?? (thread.omp?.shim ? [thread.omp.shim] : []);
			const found = roots.length > 0 ? await endProcesses(roots) : [];
			if (found.length === 0) {
				log("warn", "shutdown", `${subject(thread)}'s omp isn't known; it ends with herdr, and its turn may not go on`);
				continue;
			}
			log("info", "shutdown", `${subject(thread)}: its omp ended mid-turn, for --restore`);
		}
	}

	/**
	 * Stop everything's last step for omp (0547): the host registry entries (`~/.omp/run/collab-hosts`) of the omps
	 * Jocasta ended, once each is gone (waited for up to `HOST_END_WAIT_MS`); never another omp's.
	 */
	async removeEndedHosts(): Promise<void> {
		const ended = [
			...this.closingOmps,
			...this.c.threads.all().flatMap((thread) => thread.omp?.processes?.omp ?? []),
			...[...this.endingOmps].flatMap((set) => set.omp),
		];
		if (ended.length === 0) return;
		const gone = () => !ended.some((id) => processIdOf(id.pid)?.start === id.start);
		await waitFor(gone, { timeoutMs: HOST_END_WAIT_MS, intervalMs: HOST_END_POLL_MS });
		const removed = removeHostEntries(new Set(ended.map((id) => id.pid)), this.c.options.hostsDir);
		if (removed > 0) log("info", "shutdown", `removed ${removed} ended omp host entr${removed === 1 ? "y" : "ies"}`);
	}

	/**
	 * Each thread pane's omp as omp itself reports it (0547, 0561): its host registry entry, matched by the pid of the
	 * omp Jocasta started in the pane (`thread.omp`) and by the worktree, answers working (a turn runs), blocked (a
	 * dialog of omp's waits: a question, an approval) or idle. A change is handled as a pushed one. A thread whose omp
	 * has no entry that answers (the relay was down at its start, an omp Jocasta didn't start, no pid read yet) is
	 * unknown, never idle: herdr's events and the session file decide it as before. A plan in omp's Plan Review isn't a
	 * dialog to omp (`interactive-mode.ts` shows it without the collab host): idle here, the session file says it waits.
	 */
	poll(): Promise<void> {
		if (this.c.stopping || this.hostPoll) return this.hostPoll ?? Promise.resolve();
		this.hostPoll = (async () => {
			try {
				const byPid = new Map<number, Thread>();
				for (const thread of this.c.threads.values()) {
					for (const id of thread.omp?.processes?.omp ?? []) byPid.set(id.pid, thread);
				}
				const found = new Map<Thread, AgentState>();
				for (const host of await readHosts(new Set(byPid.keys()), this.c.options.hostsDir)) {
					const thread = byPid.get(host.pid);
					const state = hostState(host);
					if (thread && state && sameDir(host.cwd, thread.path)) found.set(thread, state);
				}
				if (this.c.stopping) return;
				for (const thread of this.c.threads.values()) {
					const state = found.get(thread);
					const was = thread.host;
					thread.host = state;
					if (state === undefined) {
						if (was !== undefined) log("info", "threads", `${subject(thread)}: omp's host is gone; herdr decides`);
						continue;
					}
					const changed = state !== (thread.state === "done" ? "idle" : thread.state);
					if (state !== was) log("info", "threads", `${subject(thread)}: omp's host reports ${state}`);
					if (changed) this.c.lifecycle.stateChanged(thread.paneId, state, "host");
				}
			} catch (error) {
				this.c.warn(error);
			} finally {
				this.hostPoll = undefined;
			}
		})();
		return this.hostPoll;
	}

	/** The host registry read still under way, for `stop` to wait for. */
	polling(): Promise<void> | undefined {
		return this.hostPoll;
	}
}
