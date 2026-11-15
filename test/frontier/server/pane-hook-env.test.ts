import { expect, test } from "bun:test";
import { join } from "node:path";
import { LaunchedPanes } from "../../../src/frontier/server/boss.ts";
import type { ServiceContext } from "../../../src/frontier/server/context.ts";
import { PaneLauncher } from "../../../src/frontier/server/pane-launch.ts";
import { selfCommand } from "../../../src/shared/install.ts";
import { PUSH_HOOK_ROLE } from "../../../src/shared/push-hook.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";
import { bossPane, startTestServer } from "./helpers.ts";

const env = useTempJocasta();

/** A launcher over a fake herdr, without mise; every pane's relay is loopback. */
function launcher(herdr: FakeHerdr): PaneLauncher {
	const c = {
		options: { omp: ["omp"], panePrompt: null, mise: null, relayUrlOf: async () => "ws://127.0.0.1:47499" },
		herdr,
		launched: new LaunchedPanes(join(env.root, "panes.json")),
		layer: true,
	} as unknown as ServiceContext;
	return new PaneLauncher(c);
}

/** The command git is configured with: this Jocasta's words, then the hook's role. */
const hookCommand = () => [...selfCommand().map((word) => `'${word.replaceAll("\\", "/")}'`), PUSH_HOOK_ROLE].join(" ");

/** Runs `fn` with GIT_CONFIG_COUNT (and none, when `count` is undefined) in the service's own environment. */
async function withGitCount<T>(count: string | undefined, fn: () => Promise<T>): Promise<T> {
	const before = process.env.GIT_CONFIG_COUNT;
	if (count === undefined) delete process.env.GIT_CONFIG_COUNT;
	else process.env.GIT_CONFIG_COUNT = count;
	try {
		return await fn();
	} finally {
		if (before === undefined) delete process.env.GIT_CONFIG_COUNT;
		else process.env.GIT_CONFIG_COUNT = before;
	}
}

test("a project or thread pane starts with the pre-push hook in git's environment; the check env has none (0613)", async () => {
	await withGitCount(undefined, async () => {
		const panes = launcher(new FakeHerdr());
		expect(panes.paneEnv).toMatchObject({
			GIT_CONFIG_COUNT: "3",
			GIT_CONFIG_KEY_0: "hook.jocasta.event",
			GIT_CONFIG_VALUE_0: "pre-push",
			GIT_CONFIG_KEY_1: "hook.jocasta.command",
			GIT_CONFIG_VALUE_1: hookCommand(),
			GIT_CONFIG_KEY_2: "hook.jocasta.enabled",
			GIT_CONFIG_VALUE_2: "true",
		});
		expect(panes.checkEnv().GIT_CONFIG_COUNT).toBeUndefined();
	});
});

test("an environment that already holds GIT_CONFIG_COUNT keeps its entries; the hook's follow (0613)", async () => {
	await withGitCount("2", async () => {
		const panes = launcher(new FakeHerdr());
		expect(panes.paneEnv).toMatchObject({
			GIT_CONFIG_COUNT: "5",
			GIT_CONFIG_KEY_2: "hook.jocasta.event",
			GIT_CONFIG_KEY_3: "hook.jocasta.command",
			GIT_CONFIG_KEY_4: "hook.jocasta.enabled",
		});
		expect(panes.paneEnv.GIT_CONFIG_KEY_0).toBeUndefined();
	});
});

test("a pane started again gets the hook and its own pane key (0613)", async () => {
	await withGitCount(undefined, async () => {
		const herdr = new FakeHerdr();
		await herdr.ensureWorkspace("w", env.root);
		const paneId = herdr.paneList.at(-1)?.paneId as string;
		await launcher(herdr).respawn(paneId, { cwd: env.root, projectId: "p-0123456789abcdef" });
		const given = herdr.of("respawn").at(-1)?.[1] as { env: Record<string, string> };
		expect(given.env.GIT_CONFIG_COUNT).toBe("3");
		expect(given.env.GIT_CONFIG_VALUE_1).toBe(hookCommand());
		expect(given.env.JOCASTA_PANE_KEY).toBeTruthy();
	});
});

test("the boss pane starts with the hook too (0613)", async () => {
	await withGitCount(undefined, async () => {
		const server = await startTestServer();
		try {
			const given = server.herdr.envOf(bossPane(server.herdr) as string);
			expect(given?.GIT_CONFIG_COUNT).toBe("3");
			expect(given?.GIT_CONFIG_VALUE_2).toBe("true");
			expect(given?.JOCASTA_PANE_KEY).toBeTruthy();
		} finally {
			await server.stop();
		}
	});
});
