import { expect, test } from "bun:test";
import { isTodayPane } from "../../../src/frontier/server/boss.ts";
import { AskPopups, agentView, PluginPanes, runAction } from "../../../src/frontier/server/plugin.ts";
import { ALLOW, allowQuestion, MERGE, Popups, START } from "../../../src/frontier/server/popups.ts";
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

test("his keys: Finish asks Merge/PR/Cancel and finishes on his choice typed with its code; New thread starts from the form; refusals throw (0544, 0554, 0568)", async () => {
	const calls: unknown[][] = [];
	const asked: string[] = [];
	const popups = new Popups();
	const answers: Record<string, { choice: string; project?: string; brief?: string; typed?: string }> = {
		"Jocasta: Finish acme-api/retry": { choice: MERGE },
		"Jocasta: New thread": { choice: START, project: "acme-api", brief: "Fix the retry." },
	};
	// Whether the answer types the popup's code, as he would; a blind one (a thread through herdr's socket) can't.
	let typesCode = true;
	const deps = {
		service: {
			keyStop: async (paneId: string | undefined) => {
				throw new UserError(`Stop: ${paneId} isn't working; nothing to stop.`);
			},
			keyReview: async (paneId: string | undefined) => `Reviewing ${paneId}`,
			keyFinishable: async (paneId: string | undefined) => ({
				paneId: `${paneId}-thread`,
				name: "acme-api/retry",
				detail: "2 commits",
			}),
			keyFinish: async (paneId: string, how: "merge" | "pr") => {
				calls.push(["finish", paneId, how]);
				return "Merged.";
			},
			newThreadForm: async () => ({ projects: ["acme-api"], project: "acme-api" }),
			keyStart: async (project: string, brief: string) => {
				calls.push(["start", project, brief]);
				return "Started.";
			},
		},
		ask: (view: Parameters<Popups["open"]>[0]) => {
			const popup = popups.open(view);
			asked.push(view.title);
			const answer = answers[view.title] ?? { choice: view.fallback };
			const tries = typesCode ? 1 : 3;
			queueMicrotask(() => {
				for (let i = 0; i < tries; i++) popups.answer(popup.code, { ...answer, typed: typesCode ? view.code : "AAA" });
			});
			return popup;
		},
		quit: async () => "kept" as const,
	};
	expect(await runAction("finish", "p1", deps)).toBe("Merged.");
	expect(calls).toEqual([["finish", "p1-thread", "merge"]]);
	expect(await runAction("new-thread", undefined, deps)).toBe("Started.");
	expect(calls[1]).toEqual(["start", "acme-api", "Fix the retry."]);
	expect(asked).toEqual(["Jocasta: Finish acme-api/retry", "Jocasta: New thread"]);
	expect(await runAction("review", "p2", deps)).toBe("Reviewing p2");
	expect(await runAction("quit", undefined, deps)).toBe("Jocasta keeps running.");
	await expect(runAction("stop", "p3", deps)).rejects.toThrow("Stop: p3 isn't working; nothing to stop.");
	// Cancel leaves the thread as it was.
	answers["Jocasta: Finish acme-api/retry"] = { choice: "Cancel" };
	expect(await runAction("finish", "p1", deps)).toBe("Finish: acme-api/retry stays as it was.");
	// Merge without its code, three times: cancelled.
	answers["Jocasta: Finish acme-api/retry"] = { choice: MERGE };
	typesCode = false;
	expect(await runAction("finish", "p1", deps)).toBe("Finish: acme-api/retry stays as it was.");
	expect(calls).toHaveLength(2);
});
