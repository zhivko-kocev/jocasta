import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { Confirmation } from "../../../src/frontier/server/allow.ts";
import { projectsFile } from "../../../src/shared/paths.ts";
import { readProjectsFile, setPushWay } from "../../../src/shared/projects-file.ts";
import type { PushAnswer, PushedRef } from "../../../src/shared/push-hook.ts";
import { GIT_TEST_TIMEOUT, sh, useTempJocasta } from "../../shared/fixture.ts";
import { commitIn, demoProject, type TestServer, withServer } from "./helpers.ts";

const env = useTempJocasta();
const NONE = "0".repeat(40);
const REMOTE = "https://user:secret@example.org/acme/demo.git";

/** What the hook sends for a push from `dir` updating `branches` (new branches by default). */
async function push(server: TestServer, dir: string, branches: string[], paneKey?: string): Promise<PushAnswer> {
	const head = sh(dir, "rev-parse", "HEAD");
	const refs: PushedRef[] = branches.map((b) => ({ ref: `refs/heads/${b}`, local: head, remote: NONE }));
	const response = await fetch(`http://127.0.0.1:${server.port}/api/push`, {
		method: "POST",
		headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
		body: JSON.stringify({ dir, remote: REMOTE, refs, ...(paneKey ? { paneKey } : {}) }),
	});
	return (await response.json()) as PushAnswer;
}

/** A test server whose Allow box answers with `answer` and keeps what it was asked. */
function asking(answer: { allow: boolean; how: "answered" | "closed" } | "never") {
	const asked: Confirmation[] = [];
	return {
		asked,
		options: {
			confirm: (ask: Confirmation) => {
				asked.push(ask);
				return answer === "never" ? new Promise<never>(() => {}) : Promise.resolve(answer);
			},
			...(answer === "never" ? { confirmWaitMs: 150 } : {}),
		},
	};
}

const pushMap = () => readProjectsFile().push;

test(
	"a repo answered direct passes unasked; one answered pull-requests is refused unasked, naming the file and line (0613)",
	async () => {
		const project = await demoProject(env.root);
		const box = asking({ allow: true, how: "answered" });
		await withServer(async (server) => {
			const file = projectsFile();
			setPushWay(project.mainPath, "direct", file);
			expect(await push(server, project.mainPath, ["main"])).toEqual({ allow: true });
			setPushWay(project.mainPath, "pull-requests", file);
			const refused = await push(server, project.mainPath, ["main", "develop"]);
			expect(refused.allow).toBe(false);
			const line = readProjectsFile().push[Object.keys(pushMap())[0] as string]?.line;
			expect(refused.reason).toContain(`${file}, line ${line}`);
			expect(refused.reason).toContain("takes pull requests");
			expect(refused.reason).toContain("main, develop");
			expect(box.asked).toEqual([]);
		}, box.options);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a push to a feature branch passes unasked and writes nothing, whatever the repo's answer (0613)",
	async () => {
		const project = await demoProject(env.root);
		const box = asking({ allow: false, how: "answered" });
		await withServer(async (server) => {
			expect(await push(server, project.mainPath, ["feature/x", "release-2"])).toEqual({ allow: true });
			expect(box.asked).toEqual([]);
			expect(pushMap()).toEqual({});
		}, box.options);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"without an answer he is asked once with the repo, branch, commits and pane, and Allow writes direct and passes (0613)",
	async () => {
		const project = await demoProject(env.root);
		commitIn(
			project.mainPath,
			"a\n",
			`Add the first thing: a really long subject line that is cut by the box ${"x".repeat(80)}`,
		);
		commitIn(project.mainPath, "b\n", "Fix the second thing");
		const box = asking({ allow: true, how: "answered" });
		await withServer(async (server) => {
			const head = sh(project.mainPath, "rev-parse", "HEAD");
			const base = sh(project.mainPath, "rev-parse", "HEAD~2");
			const response = await fetch(`http://127.0.0.1:${server.port}/api/push`, {
				method: "POST",
				headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
				body: JSON.stringify({
					dir: project.mainPath,
					remote: REMOTE,
					refs: [{ ref: "refs/heads/main", local: head, remote: base }],
				}),
			});
			expect(await response.json()).toEqual({ allow: true });
			const [ask] = box.asked;
			expect(box.asked).toHaveLength(1);
			expect(ask?.message).toBe("Push to main in demo?");
			expect(ask?.detail).toContain("A process without a Jocasta pane key asks to push to main of demo");
			expect(ask?.detail).toContain("(https://example.org/acme/demo.git)");
			expect(ask?.detail).not.toContain("secret");
			expect(ask?.detail).toContain("2 commits to main: «Fix the second thing», «Add the first thing");
			expect(ask?.detail).toContain("…»");
			expect(Object.values(pushMap()).map((e) => e.way)).toEqual(["direct"]);
			// Later pushes to the repo pass unasked.
			expect(await push(server, project.mainPath, ["develop"])).toEqual({ allow: true });
			expect(box.asked).toHaveLength(1);
		}, box.options);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"Deny writes pull-requests and refuses; later pushes are refused unasked (0613)",
	async () => {
		const project = await demoProject(env.root);
		const box = asking({ allow: false, how: "answered" });
		await withServer(async (server) => {
			const refused = await push(server, project.mainPath, ["main"]);
			expect(refused.allow).toBe(false);
			expect(refused.reason).toContain("chose Deny");
			expect(refused.reason).toContain("takes pull requests");
			expect(Object.values(pushMap()).map((e) => e.way)).toEqual(["pull-requests"]);
			expect((await push(server, project.mainPath, ["master"])).allow).toBe(false);
			expect(box.asked).toHaveLength(1);
		}, box.options);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"no answer in the wait, or a popup closed unanswered, refuses and writes nothing (0613)",
	async () => {
		const project = await demoProject(env.root);
		for (const answer of ["never", { allow: false, how: "closed" }] as const) {
			const box = asking(answer);
			await withServer(async (server) => {
				const refused = await push(server, project.mainPath, ["main"]);
				expect(refused.allow).toBe(false);
				expect(refused.reason).toContain(answer === "never" ? "No answer within" : "popup closed");
				expect(pushMap()).toEqual({});
			}, box.options);
		}
	},
	GIT_TEST_TIMEOUT,
);

test(
	"a push from a linked worktree is the main checkout's: its entry answers, and the answer is written under it (0613)",
	async () => {
		const project = await demoProject(env.root);
		const worktree = join(env.root, "demo.wt", "feature");
		sh(project.mainPath, "worktree", "add", "-q", "-b", "feature/x", worktree);
		const box = asking({ allow: true, how: "answered" });
		await withServer(async (server) => {
			expect(await push(server, worktree, ["main"])).toEqual({ allow: true });
			expect(box.asked).toHaveLength(1);
			const [key] = Object.keys(pushMap());
			expect(key?.toLowerCase()).toBe(project.mainPath.replaceAll("\\", "/").toLowerCase());
			expect(await push(server, project.mainPath, ["main"])).toEqual({ allow: true });
			expect(box.asked).toHaveLength(1);
		}, box.options);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"the project's own trunk under another name is protected; two pushes at once to a repo ask once (0613)",
	async () => {
		const project = await demoProject(env.root);
		sh(project.mainPath, "branch", "-m", "stable");
		const box = asking({ allow: true, how: "answered" });
		await withServer(async (server) => {
			expect(await push(server, project.mainPath, ["topic"])).toEqual({ allow: true });
			expect(box.asked).toEqual([]);
			const both = await Promise.all([
				push(server, project.mainPath, ["stable"]),
				push(server, project.mainPath, ["stable"]),
			]);
			expect(both).toEqual([{ allow: true }, { allow: true }]);
			expect(box.asked).toHaveLength(1);
		}, box.options);
	},
	GIT_TEST_TIMEOUT,
);

test(
	"his projects.yaml is read back whole after the answer: its other content and comments are unchanged (0613)",
	async () => {
		const project = await demoProject(env.root);
		const file = projectsFile();
		const before = readFileSync(file, "utf8");
		const box = asking({ allow: true, how: "answered" });
		await withServer(async (server) => {
			await push(server, project.mainPath, ["main"]);
			expect(readFileSync(file, "utf8").startsWith(before.trimEnd())).toBe(true);
		}, box.options);
	},
	GIT_TEST_TIMEOUT,
);
