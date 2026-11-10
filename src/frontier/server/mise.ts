/**
 * mise in project and thread panes (0473, 0477). Every pane of a project gets `MISE_ENV=jocasta-<project id>`, so mise
 * loads Jocasta's own config for that project, `~/.config/mise/config.jocasta-<project id>.override.toml` (global config,
 * trusted by path), in every worktree without a copy. With paranoid config trust on (`paranoid = ["config"]`, which
 * Jocasta's mise file sets, 0476) and every repo config in the folder trusted, `mise env --json` run there is merged
 * into the pane's env too (shims carry no `[env]`), and the review may run its `verify` (else `test`) task. The only
 * config Jocasta trusts itself is a thread worktree's unchanged copy of one the main checkout has trusted (0596): mise's
 * own worktree rule, which paranoid trust turns off. Without mise on PATH nothing changes.
 */
import { readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { asObject } from "../../shared/fsx.ts";
import { log } from "../../shared/log.ts";
import { isInside, pathKey, realPath } from "../../shared/pathkey.ts";
import { onPath, runProgram } from "../../shared/process.ts";

/** How long one mise call may take before it counts as failed. */
const MISE_TIMEOUT_MS = 15_000;

type Env = Record<string, string | undefined>;

/** mise's argv prefix: the one on PATH, or none. */
function findMise(): string[] | null {
	const found = onPath("mise");
	return found ? [found] : null;
}

/** One mise call in `cwd`; undefined when it fails or times out. */
async function runMise(mise: string[], args: string[], cwd: string, env: Env): Promise<string | undefined> {
	try {
		const run = await runProgram([...mise, ...args], { cwd, env, timeoutMs: MISE_TIMEOUT_MS });
		if (run.code === 0) return run.stdout;
		const why = run.code ?? "timed out";
		log("warn", "mise", `mise ${args.join(" ")} in ${cwd} failed (${why}): ${run.stderr.trim().split("\n")[0] ?? ""}`);
	} catch (error) {
		log("warn", "mise", `mise ${args.join(" ")} in ${cwd} failed: ${String(error)}`);
	}
	return undefined;
}

/** `MISE_ENV` with the project's `jocasta-<pid>` appended to whatever the pane already has (0477). */
function withJocastaEnv(projectId: string, base: Env): string {
	const own = `jocasta-${projectId}`;
	const names = (base.MISE_ENV ?? "")
		.split(",")
		.map((name) => name.trim())
		.filter((name) => name && name !== own);
	return [...names, own].join(",");
}

/**
 * Why mise may not load the folder's config for Jocasta (0473), or undefined when it may: paranoid config trust must
 * be on, so omp's `CI=true` can't trust a repo by itself, and `mise trust --show` must report no untrusted config here
 * or above (global config, Jocasta's own included, isn't listed: it is trusted by path). The reason is "" when there is
 * no repo config to say anything about.
 */
async function distrust(mise: string[], cwd: string, env: Env): Promise<string | undefined> {
	const [paranoid, show] = await Promise.all([
		runMise(mise, ["settings", "get", "paranoid"], cwd, env),
		runMise(mise, ["trust", "--show"], cwd, env),
	]);
	const configs = trustList(show);
	if (!isParanoid(paranoid)) return configs.length > 0 ? "mise's paranoid config trust is off" : "";
	if (show === undefined) return "mise didn't say whether its config is trusted";
	const untrusted = configs.find((config) => !config.trusted);
	return untrusted ? `${untrusted.path} isn't trusted in mise` : undefined;
}

/** Whether `mise settings get paranoid` printed paranoid config trust on. */
function isParanoid(paranoid: string | undefined): boolean {
	return paranoid !== undefined && /\btrue\b|"config"/.test(paranoid);
}

/** The configs `mise trust --show` listed, each trusted or not. */
function trustList(show: string | undefined): { path: string; trusted: boolean }[] {
	return [...(show ?? "").matchAll(/^(.+): (trusted|untrusted)\s*$/gm)].map((m) => ({
		path: m[1] as string,
		trusted: m[2] === "trusted",
	}));
}

/** Whether files `a` and `b` hold the same bytes; false when either can't be read as a file. */
function sameBytes(a: string, b: string): boolean {
	try {
		return readFileSync(a).equals(readFileSync(b));
	} catch {
		return false;
	}
}

/**
 * Trusts each config mise lists as untrusted inside the thread worktree `cwd` whose counterpart in the main checkout
 * `main` mise lists as trusted and which has the same bytes (0596). Only under paranoid config trust: without it, mise's
 * own worktree rule applies. A changed copy stays untrusted, so trust still follows content. Returns what it trusted.
 */
export async function trustCopies(
	projectId: string,
	main: string,
	cwd: string,
	base: Env = process.env,
	mise = findMise(),
): Promise<string[]> {
	if (!mise) return [];
	const env = { ...base, MISE_ENV: withJocastaEnv(projectId, base) };
	const [paranoid, here, there] = await Promise.all([
		runMise(mise, ["settings", "get", "paranoid"], cwd, env),
		runMise(mise, ["trust", "--show"], cwd, env),
		runMise(mise, ["trust", "--show"], main, env),
	]);
	if (!isParanoid(paranoid)) return [];
	const trustedInMain = new Set(
		trustList(there)
			.filter((config) => config.trusted)
			.map((config) => pathKey(realPath(config.path))),
	);
	const root = realPath(cwd);
	const mainRoot = realPath(main);
	const trusted: string[] = [];
	for (const config of trustList(here)) {
		if (config.trusted) continue;
		const copy = realPath(config.path);
		if (pathKey(copy) === pathKey(root) || !isInside(root, copy)) continue;
		const original = join(mainRoot, relative(root, copy));
		if (!trustedInMain.has(pathKey(original)) || !sameBytes(copy, original)) continue;
		if ((await runMise(mise, ["trust", copy], cwd, env)) === undefined) continue;
		log("info", "mise", `trusted ${copy}: an unchanged copy of ${original}, which mise trusts (0596)`);
		trusted.push(copy);
	}
	return trusted;
}

/**
 * The env to merge into a project or thread pane started in `cwd` (0473, 0477): `MISE_ENV` with `jocasta-<pid>`
 * appended, plus `mise env --json` when the folder's config may be loaded. `base` is the env the pane would otherwise
 * get (its thread variables reach mise's `[env]` templates). Empty when mise isn't on PATH.
 */
export async function miseEnv(
	projectId: string,
	cwd: string,
	base: Env = process.env,
	mise = findMise(),
): Promise<Record<string, string>> {
	if (!mise) return {};
	const MISE_ENV = withJocastaEnv(projectId, base);
	const env = { ...base, MISE_ENV };
	if ((await distrust(mise, cwd, env)) !== undefined) return { MISE_ENV };
	const out = await runMise(mise, ["env", "--json"], cwd, env);
	let parsed: unknown;
	try {
		parsed = out === undefined ? undefined : JSON.parse(out);
	} catch {
		log("warn", "mise", `mise env --json in ${cwd} printed no JSON`);
	}
	const merged: Record<string, string> = {};
	if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
		for (const [key, value] of Object.entries(parsed)) if (typeof value === "string") merged[key] = value;
	}
	return { ...merged, MISE_ENV };
}

/** The project's own check (0453, 0473, 0477): its mise task, or why it didn't run. */
export type MiseCheck = { task: "verify" | "test" } | { skipped: string };

/** How a project check that couldn't run is said beside the checks that did (0559): never a silent pass. */
export function skippedSaid(why: string): string {
	return `the project's check didn't run: ${why}`;
}

/**
 * The project's own check as a mise task (0453, 0473, 0477): `verify`, else `test`, when the folder's config may be
 * loaded and defines it (Jocasta's own per-project config counts through `MISE_ENV`); why it can't run when the folder
 * has a repo config mise may not load for Jocasta (0559); undefined otherwise.
 */
export async function miseCheck(
	projectId: string,
	cwd: string,
	base: Env = process.env,
	mise = findMise(),
): Promise<MiseCheck | undefined> {
	if (!mise) return undefined;
	const env = { ...base, MISE_ENV: withJocastaEnv(projectId, base) };
	const why = await distrust(mise, cwd, env);
	if (why !== undefined) return why ? { skipped: why } : undefined;
	const out = await runMise(mise, ["tasks", "ls", "--json"], cwd, env);
	const names: string[] = [];
	try {
		const tasks: unknown = out === undefined ? [] : JSON.parse(out);
		if (Array.isArray(tasks)) {
			for (const task of tasks) {
				const name = asObject(task)?.name;
				if (typeof name === "string") names.push(name);
			}
		}
	} catch {
		log("warn", "mise", `mise tasks ls --json in ${cwd} printed no JSON`);
	}
	const task = names.includes("verify") ? "verify" : names.includes("test") ? "test" : undefined;
	return task ? { task } : undefined;
}
