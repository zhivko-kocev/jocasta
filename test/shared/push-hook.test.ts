import { expect, test } from "bun:test";
import {
	isFolderRemote,
	isProtected,
	type PushAnswer,
	type PushRequest,
	parsePushedRefs,
	pushHookEnv,
	runPushHook,
} from "../../src/shared/push-hook.ts";
import { route, USAGE } from "../../src/shared/route.ts";

const SHA = "1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b";
const NONE = "0".repeat(40);

/** git's stdin for a hook: one line per updated remote ref. */
const line = (remoteRef: string, local = SHA, remote = NONE) => `refs/heads/work ${local} ${remoteRef} ${remote}\n`;

/** Runs the hook with a fake service; `calls` collects what it asked, `said` what it printed. */
async function hook(
	args: string[],
	stdin: string,
	answer: PushAnswer | string = { allow: true },
	trunk?: string,
): Promise<{ code: number; calls: PushRequest[]; said: string[] }> {
	const calls: PushRequest[] = [];
	const said: string[] = [];
	const code = await runPushHook(args, stdin, {
		cwd: "/work/acme",
		paneKey: "key-1",
		trunk: () => trunk,
		ask: async (request) => {
			calls.push(request);
			return answer;
		},
		say: (text) => said.push(text),
	});
	return { code, calls, said };
}

test("git's stdin lines become the updated refs, blank lines skipped (0613)", () => {
	expect(
		parsePushedRefs(`refs/heads/work ${SHA} refs/heads/main ${NONE}\n\nHEAD ${SHA} refs/heads/dev ${SHA}\n`),
	).toEqual([
		{ ref: "refs/heads/main", local: SHA, remote: NONE },
		{ ref: "refs/heads/dev", local: SHA, remote: SHA },
	]);
});

test("a network address is each of https, http, ssh, git, user@host:path and host:path; a path or file:// is a folder (0613)", () => {
	for (const network of [
		"https://example.org/acme/app.git",
		"http://example.org/acme/app.git",
		"ssh://git@example.org/acme/app.git",
		"git://example.org/acme/app.git",
		"git@example.org:acme/app.git",
		"example.org:acme/app.git",
	]) {
		expect([network, isFolderRemote(network)]).toEqual([network, false]);
	}
	for (const folder of [
		"/srv/git/app.git",
		"../origin.git",
		"./origin.git",
		"origin.git",
		"C:\\srv\\app.git",
		"C:/srv/app.git",
		"\\\\server\\share\\app.git",
		"file:///srv/git/app.git",
		"file://C:/srv/app.git",
	]) {
		expect([folder, isFolderRemote(folder)]).toEqual([folder, true]);
	}
});

test("main, master, develop and the project's own trunk are protected branches; feature branches and tags are not (0613)", () => {
	for (const branch of ["main", "master", "develop"]) expect(isProtected(`refs/heads/${branch}`)).toBe(true);
	expect(isProtected("refs/heads/trunk")).toBe(false);
	expect(isProtected("refs/heads/trunk", "trunk")).toBe(true);
	for (const other of [
		"refs/heads/feature/main",
		"refs/heads/maintenance",
		"refs/heads/develop2",
		"refs/tags/main",
		"refs/heads/Main",
	]) {
		expect([other, isProtected(other)]).toEqual([other, false]);
	}
});

test("a push that updates no protected branch passes without asking anything, even with the service down (0613)", async () => {
	const result = await hook(
		["origin", "https://example.org/acme/app.git"],
		line("refs/heads/feature/x"),
		"Jocasta isn't running",
	);
	expect(result).toEqual({ code: 0, calls: [], said: [] });
	expect((await hook(["origin", "https://example.org/a.git"], line("refs/tags/main"), "down")).calls).toEqual([]);
});

test("a push to a folder remote passes without asking, whatever branch it updates (0613)", async () => {
	for (const address of ["/srv/git/app.git", "file:///srv/git/app.git", "C:\\srv\\app.git"]) {
		expect(await hook(["origin", address], line("refs/heads/main"), "down")).toEqual({ code: 0, calls: [], said: [] });
	}
});

test("a push updating main, master or develop on a network remote asks the service with every updated ref and the pane's key (0613)", async () => {
	for (const branch of ["main", "master", "develop"]) {
		const stdin = line("refs/heads/feature/x") + line(`refs/heads/${branch}`, SHA, SHA);
		const result = await hook(["origin", "git@example.org:acme/app.git"], stdin);
		expect(result.code).toBe(0);
		expect(result.calls).toEqual([
			{
				dir: "/work/acme",
				remote: "git@example.org:acme/app.git",
				refs: [
					{ ref: "refs/heads/feature/x", local: SHA, remote: NONE },
					{ ref: `refs/heads/${branch}`, local: SHA, remote: SHA },
				],
				paneKey: "key-1",
			},
		]);
	}
});

test("deleting a protected branch asks too; the remote's own default branch is protected under another name (0613)", async () => {
	const del = await hook(["origin", "https://example.org/a.git"], line("refs/heads/main", NONE, SHA));
	expect(del.calls.map((c) => c.refs)).toEqual([[{ ref: "refs/heads/main", local: NONE, remote: SHA }]]);
	expect(
		(await hook(["origin", "https://example.org/a.git"], line("refs/heads/stable"), { allow: true }, "stable")).calls,
	).toHaveLength(1);
	expect(
		(await hook(["origin", "https://example.org/a.git"], line("refs/heads/stable"), { allow: true })).calls,
	).toHaveLength(0);
});

test("the service's refusal is printed for the agent and exits 1; no answer refuses with its reason (0613)", async () => {
	const refused = await hook(["origin", "https://example.org/a.git"], line("refs/heads/main"), {
		allow: false,
		reason: "acme takes pull requests.",
	});
	expect(refused).toMatchObject({ code: 1, said: ["jocasta: acme takes pull requests."] });
	const down = await hook(
		["origin", "https://example.org/a.git"],
		line("refs/heads/main"),
		"Jocasta isn't running; start it with jocasta.",
	);
	expect(down.code).toBe(1);
	expect(down.said).toEqual([
		"jocasta: this push to a protected branch is refused: Jocasta isn't running; start it with jocasta.",
	]);
});

test("the hook's environment holds the three keys; an existing GIT_CONFIG_COUNT keeps its entries and ours follow (0613)", () => {
	expect(pushHookEnv(["C:\\Program Files\\Jocasta\\jocasta.exe"], {})).toEqual({
		GIT_CONFIG_COUNT: "3",
		GIT_CONFIG_KEY_0: "hook.jocasta.event",
		GIT_CONFIG_VALUE_0: "pre-push",
		GIT_CONFIG_KEY_1: "hook.jocasta.command",
		GIT_CONFIG_VALUE_1: "'C:/Program Files/Jocasta/jocasta.exe' push-hook",
		GIT_CONFIG_KEY_2: "hook.jocasta.enabled",
		GIT_CONFIG_VALUE_2: "true",
	});
	const after = pushHookEnv(["bun", "D:/code/jocasta/src/entry.ts"], {
		GIT_CONFIG_COUNT: "2",
		GIT_CONFIG_KEY_0: "user.name",
		GIT_CONFIG_VALUE_0: "x",
	});
	expect(after).toEqual({
		GIT_CONFIG_COUNT: "5",
		GIT_CONFIG_KEY_2: "hook.jocasta.event",
		GIT_CONFIG_VALUE_2: "pre-push",
		GIT_CONFIG_KEY_3: "hook.jocasta.command",
		GIT_CONFIG_VALUE_3: "'bun' 'D:/code/jocasta/src/entry.ts' push-hook",
		GIT_CONFIG_KEY_4: "hook.jocasta.enabled",
		GIT_CONFIG_VALUE_4: "true",
	});
	expect(pushHookEnv(["jocasta"], { GIT_CONFIG_COUNT: "nope" }).GIT_CONFIG_COUNT).toBe("3");
});

test("the command's word routes to the hook with git's arguments and stays out of the usage text (0613)", () => {
	expect(USAGE).not.toContain("push-hook");
	expect(route(["push-hook", "origin", "https://example.org/a.git"])).toEqual({
		role: "push-hook",
		args: ["origin", "https://example.org/a.git"],
	});
});
