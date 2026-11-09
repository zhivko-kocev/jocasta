import { expect, test } from "bun:test";
import { isTodayPane } from "../../../src/frontier/server/boss.ts";
import { type ActionDeps, AskPopups, agentView, PluginPanes, runAction } from "../../../src/frontier/server/plugin.ts";
import {
	ALLOW,
	allowQuestion,
	finishQuestion,
	MERGE,
	type Popup,
	Popups,
	START,
} from "../../../src/frontier/server/popups.ts";
import { UserError } from "../../../src/shared/errors.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import { until } from "./helpers.ts";

useTempJocasta();

/** A boss workspace with its boss pane running omp. */
async function bossSession(): Promise<{ herdr: FakeHerdr; boss: string; workspace: string }> {
	const herdr = new FakeHerdr();
	const workspace = await herdr.ensureWorkspace("Jocasta", "/home", ["omp"]);
	const boss = herdr.paneList[0]?.paneId as string;
	herdr.setState(boss, "idle");
	return { herdr, boss, workspace };
}

test("today opens beside the boss at 65/35, is left alone while open, and opens again once gone (0553)", async () => {
	const { herdr, boss, workspace } = await bossSession();
	let now = 1_000_000;
	const panes = new PluginPanes(herdr, () => now);
	await panes.heal(await herdr.snapshot(), boss);
	const [open] = herdr.of("openPluginPane") as [{ placement: string; targetPaneId: string; entrypoint: string }][];
	expect(open?.[0]).toMatchObject({ pluginId: "jocasta", entrypoint: "today", placement: "split", targetPaneId: boss });
	const today = herdr.paneList.find((p) => p.paneId === panes.today);
	expect(today && isTodayPane(today)).toBe(true);
	expect(herdr.of("setSplitRatio")).toEqual([[today?.tabId, 0.65]]);
	// The Agents view leaves the boss's workspace out.
	expect(herdr.of("setAgentView")).toEqual([[agentView(workspace)]]);

	await panes.heal(await herdr.snapshot(), boss);
	expect(herdr.of("openPluginPane")).toHaveLength(1);
	expect(herdr.of("setAgentView")).toHaveLength(1);

	// Gone after a long life: open again at once.
	now += 60_000;
	herdr.paneList.splice(herdr.paneList.indexOf(today as never), 1);
	await panes.heal(await herdr.snapshot(), boss);
	expect(herdr.of("openPluginPane")).toHaveLength(2);
	// Gone right after opening: it waits before opening again.
	herdr.paneList.splice(
		herdr.paneList.findIndex((p) => p.paneId === panes.today),
		1,
	);
	now += 1_000;
	await panes.heal(await herdr.snapshot(), boss);
	expect(herdr.of("openPluginPane")).toHaveLength(2);
	now += 10_000;
	await panes.heal(await herdr.snapshot(), boss);
	expect(herdr.of("openPluginPane")).toHaveLength(3);
});

test("a today found in the boss's workspace after a restart is taken, never opened twice (0553)", async () => {
	const { herdr, boss } = await bossSession();
	await new PluginPanes(herdr).heal(await herdr.snapshot(), boss);
	const again = new PluginPanes(herdr);
	await again.heal(await herdr.snapshot(), boss);
	expect(herdr.of("openPluginPane")).toHaveLength(1);
	expect(again.today).toBeDefined();
});

test("questions show one at a time in the ask popup with a request notification; a popup closed unanswered denies (0558)", async () => {
	const { herdr } = await bossSession();
	const ask = new AskPopups(herdr, { holdWaitMs: 60_000, retryMs: 60_000 });
	const popups = new Popups();
	const first = popups.open(allowQuestion({ message: "Remove it?", detail: "d" }, "ABC"));
	const second = popups.open(allowQuestion({ message: "Merge it?", detail: "d" }, "DEF"));
	ask.show(first);
	ask.show(second);
	await until(() => herdr.popup !== undefined);
	expect(herdr.popup).toEqual({ JOCASTA_POPUP: first.code });
	expect(herdr.of("notify")).toEqual([[{ title: "Jocasta asks", body: "Remove it?", sound: "request" }]]);
	// Only the shown question's code holds.
	expect(ask.hold(second.code, new AbortController().signal).status).toBe(403);
	const held = new AbortController();
	expect(ask.hold(first.code, held.signal).status).toBe(200);
	expect(herdr.of("openPluginPane")).toHaveLength(1);
	// Closed some other way: the held request ends, the question denies, the next one shows.
	held.abort();
	expect(await first.answer).toBe("Deny");
	await until(() => herdr.popup?.JOCASTA_POPUP === second.code);
	const reader = ask.hold(second.code, new AbortController().signal).body?.getReader();
	expect(popups.answer(second.code, { choice: ALLOW, typed: "DEF" })).toBe("taken");
	// The answer ends the held request, so the popup closes.
	let text = "";
	for (;;) {
		const chunk = await reader?.read();
		if (!chunk || chunk.done) break;
		text += chunk.value;
	}
	expect(text).toContain("event: end");
});

test("a popup herdr couldn't open is asked for again until it shows (0553)", async () => {
	const { herdr } = await bossSession();
	let refusals = 2;
	const open = herdr.openPluginPane.bind(herdr);
	herdr.openPluginPane = async (opts) => {
		if (refusals-- > 0) throw new Error("ui_busy");
		return open(opts);
	};
	const ask = new AskPopups(herdr, { holdWaitMs: 60_000, retryMs: 5 });
	const popup = new Popups().open(allowQuestion({ message: "m", detail: "d" }));
	ask.show(popup);
	await until(() => herdr.popup !== undefined);
	expect(herdr.popup).toEqual({ JOCASTA_POPUP: popup.code });
	expect(refusals).toBe(-1);
	popup.close();
});

test("questions his own keys opened raise no notification, and Allow questions go ahead of them (0578, 0581)", async () => {
	const { herdr } = await bossSession();
	const ask = new AskPopups(herdr, { holdWaitMs: 60_000, retryMs: 60_000 });
	const popups = new Popups();
	const first = popups.open(finishQuestion("acme-api/retry", "d", "ABC"), { fromKey: true });
	ask.show(first);
	await until(() => herdr.popup?.JOCASTA_POPUP === first.code);
	const key = popups.open(finishQuestion("acme-api/other", "d", "DEF"), { fromKey: true });
	const allow = popups.open(allowQuestion({ message: "Merge it?", detail: "d" }, "GHJ"));
	ask.show(key);
	ask.show(allow);
	first.close();
	// The Allow question, though asked after the key's, shows next, with its notification.
	await until(() => herdr.popup?.JOCASTA_POPUP === allow.code);
	expect(herdr.of("notify")).toEqual([[{ title: "Jocasta asks", body: "Merge it?", sound: "request" }]]);
	allow.close();
	await until(() => herdr.popup?.JOCASTA_POPUP === key.code);
	expect(herdr.of("notify")).toHaveLength(1);
	key.close();
});

test("a question its popup read but never held counts as closed, and isn't opened again (0578)", async () => {
	const { herdr } = await bossSession();
	const ask = new AskPopups(herdr, { holdWaitMs: 20, retryMs: 20 });
	const popups = new Popups();
	const popup = popups.open(allowQuestion({ message: "m", detail: "d" }));
	ask.show(popup);
	await until(() => herdr.popup !== undefined);
	expect(popups.read(popup.code)).toHaveProperty("reader");
	expect(await popup.how).toBe("closed");
	expect(herdr.of("openPluginPane").length).toBeLessThanOrEqual(2);
});

/** His keys' deps against a fake service; `answers` answers each question by its title, typing its code or not. */
function keyDeps(answers: Record<string, { choice: string; project?: string; brief?: string }>) {
	const calls: unknown[][] = [];
	const asked: string[] = [];
	const said: string[] = [];
	const fronted: Popup[] = [];
	const popups = new Popups();
	const state = { typesCode: true, answering: true };
	const deps: ActionDeps = {
		service: {
			keyStop: async (paneId: string | undefined) => {
				throw new UserError(`Stop: ${paneId} isn't working; nothing to stop.`);
			},
			keyReview: async (paneId: string | undefined) => `Reviewing ${paneId}`,
			keyFinishable: async (paneId: string | undefined) => ({
				paneId: `${paneId}-thread`,
				name: "acme-api/retry",
				detail: "2 commits",
				trunk: "main",
			}),
			keyFinish: async (paneId: string, how: "merge" | "pr") => {
				calls.push(["finish", paneId, how]);
				return { text: "Merged.", isError: false };
			},
			newThreadForm: async (paneId?: string) => {
				calls.push(["form", paneId]);
				return { projects: ["acme-api"], project: "acme-api" };
			},
			keyStart: async (project: string, brief: string) => {
				calls.push(["start", project, brief]);
				return { text: "Started.", isError: false };
			},
		},
		ask: (view, signal) => {
			const popup = popups.open(view, { fromKey: true });
			signal.addEventListener("abort", () => popup.close(), { once: true });
			asked.push(view.title);
			if (state.answering) {
				const answer = answers[view.title] ?? { choice: view.fallback };
				const tries = state.typesCode ? 1 : 3;
				queueMicrotask(() => {
					for (let i = 0; i < tries; i++)
						popups.answer(popup.code, { ...answer, typed: state.typesCode ? view.code : "AAA" });
				});
			}
			return popup;
		},
		front: (popup) => void fronted.push(popup),
		quit: async () => "kept" as const,
		progress: (text) => void said.push(text),
		asking: new Map(),
		waitMs: 60_000,
	};
	return { deps, calls, asked, said, fronted, state };
}

test("his keys: Finish asks Merge/PR/Cancel and finishes on his choice typed with its code; New thread starts from the form; refusals throw (0544, 0554, 0568, 0581)", async () => {
	const answers: Record<string, { choice: string; project?: string; brief?: string }> = {
		"Jocasta: Finish acme-api/retry": { choice: MERGE },
		"Jocasta: New thread": { choice: START, project: "acme-api", brief: "Fix the retry." },
	};
	const { deps, calls, asked, said, state } = keyDeps(answers);
	expect(await runAction("finish", "p1", deps)).toEqual({ text: "Merged.", isError: false });
	expect(calls).toEqual([["finish", "p1-thread", "merge"]]);
	// The start line goes out at once, before the slow part (UX-5).
	expect(said).toEqual(["Merging acme-api/retry into main…"]);
	// New thread's form is for the picked pane's project (0581, C10).
	expect(await runAction("new-thread", "p9", deps)).toEqual({ text: "Started.", isError: false });
	expect(calls.slice(1)).toEqual([
		["form", "p9"],
		["start", "acme-api", "Fix the retry."],
	]);
	expect(said.at(-1)).toBe("Starting a thread in acme-api: worktree, omp, brief…");
	expect(asked).toEqual(["Jocasta: Finish acme-api/retry", "Jocasta: New thread"]);
	expect(await runAction("review", "p2", deps)).toEqual({ text: "Reviewing p2", isError: false });
	expect(await runAction("quit", undefined, deps)).toEqual({ text: "Jocasta keeps running.", isError: false });
	await expect(runAction("stop", "p3", deps)).rejects.toThrow("Stop: p3 isn't working; nothing to stop.");
	// Cancel leaves the thread as it was.
	answers["Jocasta: Finish acme-api/retry"] = { choice: "Cancel" };
	expect((await runAction("finish", "p1", deps))?.text).toBe("Finish: acme-api/retry stays as it was.");
	// Merge without its code, three times: cancelled.
	answers["Jocasta: Finish acme-api/retry"] = { choice: MERGE };
	state.typesCode = false;
	expect((await runAction("finish", "p1", deps))?.text).toBe("Finish: acme-api/retry stays as it was.");
	expect(calls.filter((c) => c[0] === "finish")).toHaveLength(1);
});

test("one open question per key and thread: a repeat brings it to the front; Finish and New thread wait as Allow does (0578)", async () => {
	const { deps, asked, fronted, state } = keyDeps({});
	state.answering = false;
	deps.waitMs = 50;
	const finishing = runAction("finish", "p1", deps);
	await until(() => asked.length === 1);
	expect(await runAction("finish", "p1", deps)).toEqual({
		text: "Finish: already asking about acme-api/retry.",
		isError: false,
	});
	const forming = runAction("new-thread", undefined, deps);
	await until(() => asked.length === 2);
	expect(await runAction("new-thread", undefined, deps)).toBeUndefined();
	expect(asked).toHaveLength(2);
	expect(fronted).toHaveLength(2);
	// Unanswered, each ends after its wait.
	expect((await finishing)?.text).toBe("Finish: acme-api/retry stays as it was.");
	expect((await forming)?.text).toBe("New thread: none started.");
	expect(deps.asking.size).toBe(0);
});
