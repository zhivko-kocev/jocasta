import { expect, test } from "bun:test";
import { join } from "node:path";
import { LaunchedPanes } from "../../../src/frontier/server/boss.ts";
import type { ServiceContext } from "../../../src/frontier/server/context.ts";
import { overlayFile } from "../../../src/frontier/server/overlay.ts";
import { PaneLauncher, THREAD_FLAGS } from "../../../src/frontier/server/pane-launch.ts";
import { newTask, taskDir } from "../../../src/frontier/server/task-file.ts";
import type { Thread } from "../../../src/frontier/server/thread.ts";
import { shippedConfigDir } from "../../../src/shared/install.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import { GUARD } from "./helpers.ts";

const env = useTempJocasta();

/** A launcher over a fake herdr with no shipped prompt; every pane's relay is loopback. */
function launcher(herdr: FakeHerdr): PaneLauncher {
	const c = {
		options: { omp: ["omp"], panePrompt: null, relayUrlOf: async () => "ws://127.0.0.1:47499" },
		herdr,
		launched: new LaunchedPanes(join(env.root, "panes.json")),
		layer: true,
	} as unknown as ServiceContext;
	return new PaneLauncher(c);
}

test("a heavy thread's omp starts again with heavy.yml after the overlay, resuming or restoring; a thread that isn't heavy, without (0590)", async () => {
	const herdr = new FakeHerdr();
	const panes = launcher(herdr);
	const cwd = join(env.root, "demo.wt", "migrate");
	const projectId = "p-0123456789abcdef";
	const task = newTask({
		projectId,
		project: "demo",
		title: "Migrate",
		brief: "Migrate it.",
		doneWhen: [],
		heavy: true,
	});
	const thread = { task } as unknown as Thread;
	const respawned = async (how: { restore?: string; thread: Thread }) => {
		await herdr.ensureWorkspace(`w${herdr.calls.length}`, cwd);
		const paneId = herdr.paneList.at(-1)?.paneId as string;
		await panes.respawn(paneId, { cwd, projectId, env: {}, ...how });
		// FakeHerdr records `respawn`'s options as given: `{ cwd, command, env }`.
		const given = herdr.of("respawn").at(-1)?.[1] as { command: string[] } | undefined;
		return given?.command;
	};
	const prompt = join(taskDir(task), "prompt.md");
	const heavy = ["--config", join(shippedConfigDir(), "heavy.yml")];
	const overlay = ["--config", overlayFile(projectId)];
	expect(await respawned({ thread })).toEqual([
		"omp",
		"--continue",
		"--append-system-prompt",
		prompt,
		...GUARD,
		...THREAD_FLAGS,
		...overlay,
		...heavy,
	]);
	const session = join(env.root, "s.jsonl");
	expect(await respawned({ thread, restore: session })).toEqual([
		"omp",
		"--append-system-prompt",
		prompt,
		...GUARD,
		...THREAD_FLAGS,
		"--restore",
		session,
		...overlay,
		...heavy,
	]);
	const light = { task: { ...task, heavy: undefined } } as unknown as Thread;
	expect(await respawned({ thread: light })).not.toContain(heavy[1]);
});
