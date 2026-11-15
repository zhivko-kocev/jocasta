// The git `pre-push` hook Jocasta puts in every pane's environment (0613): which pushes need an answer, the words git
// runs, and what the hook says to the Jocasta service. The service's side is `frontier/server/push-gate.ts`.

/** The internal word of the `jocasta` command git runs as the hook (0613); not in the usage text. */
export const PUSH_HOOK_ROLE = "push-hook";

/** Where the hook asks the service (`POST`, the API token as the bearer). */
export const PUSH_PATH = "/api/push";

/** The all-zero commit git gives for a ref that doesn't exist yet (a new branch) or is being deleted. */
export const NO_COMMIT = /^0+$/;

/** The branches no pane pushes to without an answer (0613); the project's own trunk is another when it is named otherwise. */
const PROTECTED = ["main", "master", "develop"];

/** One line git gives a `pre-push` hook on stdin: a ref the push updates. */
export interface PushedRef {
	/** The remote's ref, `refs/heads/main`. */
	ref: string;
	/** The commit the push puts there; all zeros when the push deletes the ref. */
	local: string;
	/** The commit the ref has now; all zeros when it is new. */
	remote: string;
}

/** What the hook sends the service (`PUSH_PATH`). */
export interface PushRequest {
	/** The folder the push runs in, a checkout or worktree. */
	dir: string;
	/** The remote's address as git gave it. */
	remote: string;
	refs: PushedRef[];
	/** The pane's `JOCASTA_PANE_KEY`, which the hook inherits from its pane. */
	paneKey?: string;
}

/** The service's answer: pass, or refuse with a reason for the agent. */
export interface PushAnswer {
	allow: boolean;
	reason?: string;
}

/** The updated refs of a hook's stdin: `<local ref> <local sha> <remote ref> <remote sha>` a line. */
export function parsePushedRefs(stdin: string): PushedRef[] {
	const refs: PushedRef[] = [];
	for (const line of stdin.split(/\r?\n/)) {
		const [, local, ref, remote] = line.trim().split(/\s+/);
		if (local && ref && remote) refs.push({ ref, local, remote });
	}
	return refs;
}

/**
 * Whether `address` is a folder on this disk: a path or `file://`. Everything git would reach over a network
 * (`https://`, `http://`, `ssh://`, `git://`, `user@host:path`, `host:path`) is not: git's own rule, a colon before
 * any slash, except a drive letter.
 */
export function isFolderRemote(address: string): boolean {
	if (/^file:\/\//i.test(address)) return true;
	const colon = address.indexOf(":");
	if (colon <= 0) return true;
	if (colon === 1 && /[A-Za-z]/.test(address[0] ?? "")) return true;
	const before = address.slice(0, colon);
	return before.includes("/") || before.includes("\\");
}

/** Whether `ref` is a protected branch: main, master, develop, or `trunk` (the project's own, when named otherwise). */
export function isProtected(ref: string, trunk?: string): boolean {
	const branch = ref.startsWith("refs/heads/") ? ref.slice("refs/heads/".length) : undefined;
	return branch !== undefined && (PROTECTED.includes(branch) || branch === trunk);
}

/** `path` as git's shell reads it on this system: forward slashes, in single quotes. */
function shellWord(path: string): string {
	return `'${path.replaceAll("\\", "/").replaceAll("'", "'\\''")}'`;
}

/**
 * The environment that gives git the hook (0613): `GIT_CONFIG_COUNT` and its `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n`
 * pairs for `hook.jocasta.*`, added after the entries `base` already holds. `command` is how `jocasta` starts
 * (`selfCommand()`); git adds the remote's name and address.
 */
export function pushHookEnv(
	command: string[],
	base: Record<string, string | undefined> = process.env,
): Record<string, string> {
	const held = Number.parseInt(base.GIT_CONFIG_COUNT ?? "", 10);
	const first = Number.isInteger(held) && held > 0 ? held : 0;
	const entries = [
		["hook.jocasta.event", "pre-push"],
		["hook.jocasta.command", [...command.map(shellWord), PUSH_HOOK_ROLE].join(" ")],
		["hook.jocasta.enabled", "true"],
	] as const;
	const env: Record<string, string> = { GIT_CONFIG_COUNT: String(first + entries.length) };
	entries.forEach(([key, value], i) => {
		env[`GIT_CONFIG_KEY_${first + i}`] = key;
		env[`GIT_CONFIG_VALUE_${first + i}`] = value;
	});
	return env;
}

/** What the hook needs from outside. */
export interface PushHookDeps {
	/** The folder the push runs in. */
	cwd: string;
	/** The pane's key, when its environment has one. */
	paneKey?: string;
	/** The branch the remote called `remote` has as its default (`origin/HEAD`), when git knows it. */
	trunk(remote: string): string | undefined;
	/** Asks the service; a string is why it didn't answer. */
	ask(request: PushRequest): Promise<PushAnswer | string>;
	/** A line to stderr, for the agent. */
	say(line: string): void;
}

/**
 * The hook's verdict as its exit code (0613): `args` are the remote's name and address. A push that updates no
 * protected branch, or goes to a folder on this disk, passes without asking anything; the hook knows the three fixed
 * names and the remote's default branch, and the service adds the project's own trunk to every push it is sent. No
 * answer from the service refuses.
 */
export async function runPushHook(args: string[], stdin: string, deps: PushHookDeps): Promise<number> {
	const [name = "", address = name] = args;
	if (isFolderRemote(address)) return 0;
	const refs = parsePushedRefs(stdin);
	const trunk = deps.trunk(name);
	if (!refs.some((r) => isProtected(r.ref, trunk))) return 0;
	const answer = await deps.ask({
		dir: deps.cwd,
		remote: address,
		refs,
		...(deps.paneKey ? { paneKey: deps.paneKey } : {}),
	});
	if (typeof answer === "string") {
		deps.say(`jocasta: this push to a protected branch is refused: ${answer}`);
		return 1;
	}
	if (answer.allow) return 0;
	deps.say(`jocasta: ${answer.reason ?? "this push to a protected branch is refused."}`);
	return 1;
}
