import { mkdirSync } from "node:fs";
import { join } from "node:path";
import type {
	AgentState,
	Herdr,
	HerdrSnapshot,
	NotificationResult,
	PaneInfo,
} from "../../../src/frontier/server/herdr.ts";
import { sh } from "../../shared/fixture.ts";

/** One recorded herdr call. */
export interface HerdrCall {
	method: string;
	args: unknown[];
}

/**
 * An in-memory herdr for the service's tests: workspaces, panes and agent states, every call recorded. `worktree`
 * runs a real `git worktree add` so the service sees a linked worktree. Like herdr's, `ensureWorkspace` lists, then
 * creates: two calls at once both create (CR-10).
 */
export class FakeHerdr implements Herdr {
	readonly calls: HerdrCall[] = [];
	readonly workspaceList: { id: string; label: string; cwd: string }[] = [];
	readonly paneList: PaneInfo[] = [];
	/** Each pane's root process (`paneRoot`), as a test sets it; none by default. */
	readonly roots = new Map<string, number>();
	/** What each pane shows (`screen`), as a test sets it; empty by default. */
	readonly screens = new Map<string, string>();
	/** Whether Jocasta started this session's server (`connectHerdr`, 0495); a test of an existing session clears it. */
	startedServer = true;
	/**
	 * The state a worktree's new pane reports at once, with herdr's event, as an omp that comes up before the service
	 * first looks (TST-2); undefined: none until a test sets one, so `agentUp`'s polling stays covered.
	 */
	ompUpOnCreate: AgentState | undefined;
	/** Thrown by `createWorktree` once it made the worktree, workspace and pane, as herdr's layout failing would. */
	createWorktreeFails: Error | undefined;
	/** Each pane's env as it was started (its pane key among it, 0578). */
	private readonly envs = new Map<string, Record<string, string>>();
	private next = 0;
	private terminals = 0;
	private watcher: ((event: { paneId: string; workspaceId: string; state: AgentState }) => void) | undefined;

	/** The env `paneId` was started with; undefined for a pane started without one. */
	envOf(paneId: string): Record<string, string> | undefined {
		return this.envs.get(paneId);
	}

	private id(prefix: string): string {
		return `${prefix}_${++this.next}`;
	}

	private record(method: string, ...args: unknown[]): void {
		this.calls.push({ method, args });
	}

	/** The calls of one method, in order. */
	of(method: string): unknown[][] {
		return this.calls.filter((c) => c.method === method).map((c) => c.args);
	}

	private addPane(
		workspaceId: string,
		cwd: string,
		command?: string[],
		tabId?: string,
		env?: Record<string, string>,
	): PaneInfo {
		const paneId = this.id("p");
		const pane: PaneInfo = { paneId, tabId: tabId ?? this.id("t"), workspaceId, cwd, agentState: "unknown" };
		pane.terminalId = `term_${++this.terminals}`;
		if (command) pane.label = command.join(" ");
		if (env) this.envs.set(paneId, env);
		this.paneList.push(pane);
		return pane;
	}

	/**
	 * Changes a pane's agent state, as omp's integration would report it (omp is the pane's agent once it reports).
	 * `push: false` changes it without the event, as when herdr's event was lost (CR-09).
	 */
	setState(paneId: string, state: AgentState, sessionPath?: string, push = true): void {
		const pane = this.paneList.find((p) => p.paneId === paneId);
		if (!pane) throw new Error(`no pane ${paneId}`);
		pane.agentState = state;
		if (state === "unknown") delete pane.agent;
		else pane.agent = "omp";
		if (sessionPath) pane.sessionPath = sessionPath;
		if (push) this.watcher?.({ paneId, workspaceId: pane.workspaceId, state });
	}

	/** herdr restarted and restored every pane as a shell: same pane ids, new terminals, no agent (0404). */
	restore(): void {
		for (const pane of this.paneList) {
			pane.agentState = "unknown";
			delete pane.agent;
			delete pane.sessionPath;
			pane.terminalId = this.id("term");
			pane.label = "shell";
		}
	}

	/** He closes a workspace in herdr (or the service does): its panes go; a worktree checkout stays on disk. */
	async closeWorkspace(workspaceId: string): Promise<void> {
		this.record("closeWorkspace", workspaceId);
		this.workspaceList.splice(
			this.workspaceList.findIndex((w) => w.id === workspaceId),
			1,
		);
		for (let i = this.paneList.length - 1; i >= 0; i--) {
			if (this.paneList[i]?.workspaceId === workspaceId) this.paneList.splice(i, 1);
		}
	}

	async ping(): Promise<string | undefined> {
		return "0.9.4";
	}

	async ensureWorkspace(label: string, cwd: string, command?: string[], env?: Record<string, string>): Promise<string> {
		this.record("ensureWorkspace", label, cwd, command, ...(env ? [env] : []));
		const existing = this.workspaceList.find((w) => w.label === label);
		// herdr answers workspace.list, then workspace.create: another caller can run in between.
		await Bun.sleep(0);
		if (existing) return existing.id;
		const id = this.id("w");
		this.workspaceList.push({ id, label, cwd });
		this.addPane(id, cwd, command, undefined, env);
		return id;
	}

	async workspaces(): Promise<{ id: string; label: string; cwd?: string }[]> {
		return this.workspaceList.map((w) => ({ ...w }));
	}

	async snapshot(): Promise<HerdrSnapshot> {
		return { workspaces: await this.workspaces(), panes: await this.panes() };
	}

	async createTab(
		workspaceId: string,
		tab: { title: string; cwd: string; command: string[]; env?: Record<string, string> },
	): Promise<{ tabId: string; paneId: string }> {
		this.record("createTab", workspaceId, tab);
		const pane = this.addPane(workspaceId, tab.cwd, tab.command, undefined, tab.env);
		return { tabId: pane.tabId, paneId: pane.paneId };
	}

	async createWorktree(
		workspaceId: string,
		opts: {
			repo: string;
			branch: string;
			base?: string;
			path: string;
			title: string;
			command: string[];
			env?: Record<string, string>;
			prepare?(path: string): Promise<Record<string, string>>;
		},
	): Promise<{ workspaceId: string; tabId: string; paneId: string; path: string }> {
		const { path } = opts;
		mkdirSync(join(path, ".."), { recursive: true });
		// Like herdr: checks out `branch` at `path`, creating it from `base` (else HEAD) when missing.
		const exists = Bun.spawnSync(["git", "-C", opts.repo, "rev-parse", "--verify", "--quiet", opts.branch]).success;
		if (exists) sh(opts.repo, "worktree", "add", path, opts.branch);
		else sh(opts.repo, "worktree", "add", "-b", opts.branch, path, ...(opts.base ? [opts.base] : []));
		const extra = opts.prepare ? await opts.prepare(path) : {};
		const { prepare: _, ...recorded } = opts;
		this.record("createWorktree", workspaceId, { ...recorded, env: { ...opts.env, ...extra } });
		const id = this.id("w");
		this.workspaceList.push({ id, label: opts.title, cwd: path });
		const pane = this.addPane(id, path, opts.command, undefined, { ...opts.env, ...extra });
		if (this.createWorktreeFails) throw this.createWorktreeFails;
		if (this.ompUpOnCreate) this.setState(pane.paneId, this.ompUpOnCreate);
		return { workspaceId: id, tabId: pane.tabId, paneId: pane.paneId, path };
	}

	async moveWorkspace(workspaceId: string, index: number): Promise<void> {
		this.record("moveWorkspace", workspaceId, index);
		const at = this.workspaceList.findIndex((w) => w.id === workspaceId);
		const [moved] = this.workspaceList.splice(at, 1);
		if (moved) this.workspaceList.splice(index, 0, moved);
	}

	async respawn(
		paneId: string,
		opts: { cwd: string; command: string[]; env?: Record<string, string> },
	): Promise<{ tabId: string; paneId: string }> {
		this.record("respawn", paneId, opts);
		const target = this.paneList.find((p) => p.paneId === paneId);
		if (!target) throw new Error(`no pane ${paneId}`);
		for (let i = this.paneList.length - 1; i >= 0; i--) {
			if (this.paneList[i]?.tabId === target.tabId) this.paneList.splice(i, 1);
		}
		const pane = this.addPane(target.workspaceId, opts.cwd, opts.command, target.tabId, opts.env);
		return { tabId: pane.tabId, paneId: pane.paneId };
	}

	async splitBeside(paneId: string, opts: { command: string[]; cwd: string; label?: string }): Promise<string> {
		this.record("splitBeside", paneId, opts);
		const target = this.paneList.find((p) => p.paneId === paneId);
		const pane = this.addPane(target?.workspaceId ?? "", opts.cwd, opts.command, target?.tabId);
		if (opts.label) pane.label = opts.label;
		return pane.paneId;
	}

	async prompt(paneId: string, text: string): Promise<void> {
		this.record("prompt", paneId, text);
	}

	async sendText(paneId: string, text: string): Promise<void> {
		this.record("sendText", paneId, text);
	}

	async stop(paneId: string): Promise<void> {
		this.record("stop", paneId);
	}

	async submit(paneId: string): Promise<void> {
		this.record("submit", paneId);
	}

	async sendKeys(paneId: string, keys: string[]): Promise<void> {
		this.record("sendKeys", paneId, keys);
	}

	/** What `notification.show` answers; `no_foreground_client` while no client is attached. */
	notifyResult: NotificationResult = "shown";

	async notify(note: { title: string; body?: string; sound: "request" | "none" }): Promise<NotificationResult> {
		this.record("notify", note);
		return this.notifyResult;
	}

	async focus(paneId: string): Promise<void> {
		this.record("focus", paneId);
	}

	/** The open popup's env, as `openPluginPane` got it; undefined while none is open. */
	popup: Record<string, string> | undefined;

	async openPluginPane(opts: {
		pluginId: string;
		entrypoint: string;
		placement: "split" | "popup";
		targetPaneId?: string;
		cwd?: string;
		env?: Record<string, string>;
	}): Promise<string | undefined> {
		this.record("openPluginPane", opts);
		if (opts.placement === "popup") {
			this.popup = opts.env ?? {};
			return undefined;
		}
		const target = this.paneList.find((p) => p.paneId === opts.targetPaneId);
		if (!target) throw new Error(`no pane ${opts.targetPaneId}`);
		const pane = this.addPane(target.workspaceId, opts.cwd ?? "", undefined, target.tabId);
		pane.label = opts.entrypoint;
		return pane.paneId;
	}

	async closePopup(): Promise<boolean> {
		this.record("closePopup");
		const open = this.popup !== undefined;
		this.popup = undefined;
		return open;
	}

	async setSplitRatio(tabId: string, ratio: number): Promise<void> {
		this.record("setSplitRatio", tabId, ratio);
	}

	async setAgentView(view: object): Promise<void> {
		this.record("setAgentView", view);
	}

	async screen(paneId: string): Promise<string> {
		this.record("screen", paneId);
		return this.screens.get(paneId) ?? "";
	}

	async reportPaneMetadata(paneId: string, meta: unknown): Promise<void> {
		this.record("reportPaneMetadata", paneId, meta);
	}

	async reportWorkspaceMetadata(workspaceId: string, tokens: Record<string, string | null>): Promise<void> {
		this.record("reportWorkspaceMetadata", workspaceId, tokens);
	}

	async paneRoot(paneId: string): Promise<number | undefined> {
		return this.roots.get(paneId);
	}

	async panes(): Promise<PaneInfo[]> {
		return this.paneList.map((p) => ({ ...p }));
	}

	watch(
		_paneIds: () => string[],
		onChange: (event: { paneId: string; workspaceId: string; state: AgentState }) => void,
	): () => void {
		this.watcher = onChange;
		return () => {
			this.watcher = undefined;
		};
	}

	keepDown(): void {
		this.record("keepDown");
	}

	/** Its server stops with every pane (0434). */
	async stopServer(): Promise<void> {
		this.record("stopServer");
		this.workspaceList.splice(0);
		this.paneList.splice(0);
	}

	close(): void {
		this.watcher = undefined;
	}
}
