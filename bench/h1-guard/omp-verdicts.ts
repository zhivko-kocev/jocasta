// H1 (0482, 0500, 0501, 0535, 0600, 0623, 0636): the oracle, omp 19.1.1's own code. It is the one TypeScript file of the
// bench that stays after the Rust port (0636: "H1's oracle stays a TypeScript file, run with Bun"), because omp's modules
// load only on Bun. It finds and loads omp from the installed sources, judges a command through omp's `BashTool`
// approval and `resolveApproval` with nothing executed, and runs a line through omp's `executeBash`. It imports node
// and omp's package only. The script around it (the corpus, the rows, the summary, the git guard's reimplementation and
// the comparison with the evaluator) is `jocasta-bench h1-guard omp-verdicts`, which starts this file with Bun;
// `test/config/guard.test.ts` speaks the same protocol.
//
//   bun bench/h1-guard/omp-verdicts.ts
//
// How omp decides before a pane's bash tool runs a command (omp 19.1.1, `@oh-my-pi/pi-coding-agent/src`):
// - `extensibility/extensions/wrapper.ts:285-306,373-390` wraps every tool: `resolveApprovalFromContext`
//   (`tools/approval.ts:96-112`) takes the mode from `tools.approvalMode` (default `yolo`, `tools/settings.ts:313-317`;
//   Jocasta's guard and overlay set none) and `resolveApproval` decides, once on the call as made and again on the
//   input an extension may have revised; deny throws before the tool runs, prompt asks (`:389-525`; with no UI, as in
//   `omp -p`, it throws instead, `:462-485`); then the tool executes (`:527-541`).
// - `tools/approval.ts:356-492`: the tool's own decision first; a tool `deny` wins; under `yolo` a tool `policy` holds
//   and everything else is allowed (so a decision with only `override`, omp's critical patterns, asks nothing);
//   under other modes an override asks and the mode's tier decides the rest; a `tools.restrictedFolders` match then
//   turns anything but a deny into a prompt.
// - `tools/bash.ts:834-894` (`BashTool.approval`): the bash interceptor first (`bashInterceptor`, off by default and in
//   Jocasta's guard; it acts only for tools in the call's `toolNames`) refuses a call it intercepts; then
//   `bash.patterns` (`:254-461`: a line without shell control takes the first rule in list order that matches the
//   whole normalized command; on a line with shell control allow rules never apply, and a deny or prompt rule matches
//   the whole line or any segment from `tools/shell-tokenize.ts:14` `tokenizeShellSegments`, which honours and
//   removes quotes and splits at `; && || | & ( )` and newlines, any deny winning over any prompt; since 18.20.1 a
//   segment is also matched with its leading `NAME=value` words removed); a deny from the operator's layers holds
//   whatever a project layer says (`grantAuthority`); then `CRITICAL_BASH_PATTERNS` (`:199-252`) as an override
//   without a policy, checked before prompt rules; then prompt rules. `bash.allowCompoundCommands` (default off)
//   switches, under a POSIX shell, to per-`&&` segments. Since 18.20.1 `bash.patterns` from several layers and
//   `--config` files are joined, the higher one's first, where before the last one replaced the rest.
// - `bash.gitGuard` isn't an approval: the embedded shell's `git` builtin refuses at run time
//   (`exec/bash-executor.ts:867-869`). The oracle's judge therefore answers omp's approval alone; the git guard's
//   refusals (measured, or the reimplementation of `git-guard.ts`) are combined with it on the Rust side.
//
// The mechanism: it imports omp's `config/settings.ts`, `tools/bash.ts` and `tools/approval.ts` from the install,
// loads the guard as `--config` over omp's defaults (an empty agent dir and cwd, so no user or project config), builds
// the real `BashTool` on a session stub that throws on anything but `settings`, and calls omp's `resolveApproval` as the
// wrapper does, without a tool context: the bash tool's decision reads the context only for `toolNames`, which only the
// interceptor uses, and the guard leaves the interceptor off. `execute` is never called by the judge.
//
// Run with Bun it speaks JSON lines: one request object per line on stdin; each answer is one line on stdout that starts
// with ORACLE_MARK (omp's own code may print to stdout too; the client ignores every line without the mark):
// `{"ok": <result>}` or `{"error": <message>, "usage": <bool>}` (usage: a refusal that is the operator's to fix, the
// client's exit 2). Requests:
//   {"op":"find"}                                                        → the package dir or null (`findOmpDir`)
//   {"op":"load","omp":<package dir>|null,"anyVersion":bool}              → {dir, version, clientTerminalRoute, permissionKinds}
//   {"op":"tokenize","command"}                                          → string[][] (omp's `tokenizeShellSegments`)
//   {"op":"judge-init","configFiles":[…],"cwd","agentDir"?}              → {summary, state} (see `ompApprovals`)
//   {"op":"judge","command"}                                             → omp's approval for the line (`Approval`)
//   {"op":"exec-init","temp","env"}                                      → null (settings from an empty agent folder)
//   {"op":"exec","command","cwd","timeout","gitGuard","env"}             → {exitCode, timedOut, output} (`executeBash`)
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

export const PINNED_OMP = "19.1.1";
const PACKAGE = "@oh-my-pi/pi-coding-agent";

/** What the operator is to fix (a wrong package, a wrong version), as opposed to a fault of the oracle's own. */
export class OracleUsageError extends Error {}

export type Verdict = "allow" | "deny" | "prompt";
type Decision = string | { tier: string; reason?: string; override?: boolean; policy?: Verdict };
interface Resolved {
	policy: Verdict;
	source?: "tool" | "user" | "mode" | "restriction";
	reason?: string;
}
interface ApprovalSubject {
	approval(args: unknown): Decision;
}
interface Setting<T> {
	get(settings: unknown): T;
}

/** The omp modules H1 calls, loaded from an installed package. */
export interface Omp {
	dir: string;
	version: string;
	Settings: {
		loadReadOnly(options: { cwd?: string; agentDir?: string; configFiles?: string[] }): Promise<unknown>;
	};
	BashTool: new (session: unknown) => ApprovalSubject;
	resolveApproval(
		tool: ApprovalSubject,
		args: unknown,
		mode: string,
		userConfig: Record<string, unknown>,
		grants: undefined,
		restrictions: unknown,
	): Resolved;
	resolveApprovalFromContext(context: { settings: unknown }): {
		approvalMode: string;
		userPolicies: Record<string, unknown>;
	};
	approvalRestrictions(settings: unknown, cwd: string): unknown;
	tokenizeShellSegments(command: string): string[][];
	cfgBashGitGuard: Setting<boolean>;
	cfgBashPatterns: Setting<unknown[]>;
	cfgBashAllowCompoundCommands: Setting<boolean>;
	cfgBashInterceptorEnabled: Setting<boolean>;
	cfgLaunchEnabled: Setting<boolean>;
	cfgAsyncEnabled: Setting<boolean>;
	/** `tools.approvalMode`; `isConfigured` tells an explicit `yolo` from the schema's default (ACP asks for the latter). */
	cfgToolsApprovalMode: Setting<string> & { isConfigured(settings: unknown): boolean };
	/** The bash tool can run a call in an ACP client's terminal (`tools/bash.ts` `wrapShellLineForClientTerminal`). */
	clientTerminalRoute: boolean;
	/** The options omp's ACP permission gate offers (`session/acp-permission-gate.ts`). */
	acpPermissionOptions: { optionId: string; kind: string }[];
}

/** omp's version when `dir` holds its package, else undefined. */
function ompPackageVersion(dir: string): string | undefined {
	const manifest = join(dir, "package.json");
	if (!existsSync(manifest)) return undefined;
	const parsed: unknown = JSON.parse(readFileSync(manifest, "utf8"));
	if (typeof parsed !== "object" || parsed === null || !("name" in parsed) || parsed.name !== PACKAGE) return undefined;
	return "version" in parsed && typeof parsed.version === "string" ? parsed.version : undefined;
}

/** omp's package folder: from `omp` on PATH (npm's or Bun's global layout), else Bun's global install. */
export function findOmpDir(): string | undefined {
	const candidates: string[] = [];
	const onPath = Bun.which("omp");
	if (onPath) {
		const real = realpathSync(onPath);
		// npm: <prefix>/omp(.cmd) beside node_modules; Bun: ~/.bun/bin/omp(.exe) beside ~/.bun/install/global.
		candidates.push(join(dirname(onPath), "node_modules", PACKAGE));
		candidates.push(join(dirname(real), "..", "install", "global", "node_modules", PACKAGE));
		for (let dir = dirname(real); dirname(dir) !== dir; dir = dirname(dir)) candidates.push(dir);
	}
	candidates.push(
		join(process.env.BUN_INSTALL ?? join(homedir(), ".bun"), "install", "global", "node_modules", PACKAGE),
	);
	return candidates.map((dir) => resolve(dir)).find((dir) => ompPackageVersion(dir) !== undefined);
}

/** Imports omp's decision modules from `dir`; anything but the pinned version is refused unless `anyVersion`. */
export async function loadOmp(dir: string, anyVersion = false): Promise<Omp> {
	const version = ompPackageVersion(dir);
	if (version === undefined) throw new OracleUsageError(`${dir} is not omp's package (${PACKAGE})`);
	if (version !== PINNED_OMP && !anyVersion)
		throw new OracleUsageError(`omp ${version} at ${dir}; H1 runs on ${PINNED_OMP} (0535), or pass --any-version`);
	const load = (path: string) => import(pathToFileURL(join(dir, "src", path)).href);
	const [settings, bash, approval, tokenize, exec, tools, gate] = await Promise.all([
		load("config/settings.ts"),
		load("tools/bash.ts"),
		load("tools/approval.ts"),
		load("tools/shell-tokenize.ts"),
		load("exec/settings.ts"),
		load("tools/settings.ts"),
		load("session/acp-permission-gate.ts"),
	]);
	return {
		dir,
		version,
		Settings: settings.Settings,
		BashTool: bash.BashTool,
		resolveApproval: approval.resolveApproval,
		resolveApprovalFromContext: approval.resolveApprovalFromContext,
		approvalRestrictions: approval.approvalRestrictions,
		tokenizeShellSegments: tokenize.tokenizeShellSegments,
		cfgBashGitGuard: exec.cfgBashGitGuard,
		cfgBashPatterns: exec.cfgBashPatterns,
		cfgBashAllowCompoundCommands: exec.cfgBashAllowCompoundCommands,
		cfgBashInterceptorEnabled: exec.cfgBashInterceptorEnabled,
		cfgLaunchEnabled: tools.cfgLaunchEnabled,
		cfgAsyncEnabled: tools.cfgAsyncEnabled,
		cfgToolsApprovalMode: tools.cfgToolsApprovalMode,
		clientTerminalRoute: typeof bash.wrapShellLineForClientTerminal === "function",
		acpPermissionOptions: Array.isArray(gate.PERMISSION_OPTIONS) ? gate.PERMISSION_OPTIONS : [],
	};
}

/** omp's approval for one line (before the git guard, which is no approval). */
export interface Approval {
	/** omp's `resolveApproval` policy. */
	approval: Verdict;
	/** guard-pattern, critical-pattern, mode-default, user-policy, restricted-folder or tool. */
	mechanism: string;
	/** The guard pattern that decided, or null. */
	match: string | null;
	/** omp's `CRITICAL_BASH_PATTERNS` flagged the line (under `yolo` that asks nothing). */
	critical: boolean;
}

export interface OmpSettingsSummary {
	approvalMode: string;
	userPolicies: string[];
	gitGuard: boolean;
	allowCompoundCommands: boolean;
	patterns: number;
	restrictedFolders: boolean;
}

const PATTERN_REASON = /^(?:Blocked by|Prompt required by) bash pattern: (.*)$/s;

/**
 * Loads settings as a pane would (`configFiles` as `--config`, in order) and returns omp's judge for one command.
 * `agentDir` unset means omp's own (his global config); H1 passes an empty folder so only omp's defaults and the
 * guard apply.
 */
export async function ompApprovals(
	omp: Omp,
	options: { configFiles: string[]; cwd: string; agentDir?: string },
): Promise<{
	settings: unknown;
	summary: OmpSettingsSummary;
	judge: (command: string) => Approval;
}> {
	delete process.env.PI_CONFIG_FILES;
	const settings = await omp.Settings.loadReadOnly(options);
	// The real BashTool; its approval reads `session.settings`, and since 19.1.0 `session.toolCommands` (the session's tool
	// commands, `exec/tool-command-host.ts` `toolCommandNames`), which is absent here: a corpus line is plain bash, never a
	// tool command. Anything else means a path towards execution.
	const session = new Proxy(
		{},
		{
			get(_target, property) {
				if (property === "settings") return settings;
				if (property === "toolCommands") return undefined;
				if (property === "then") return undefined;
				throw new Error(`H1: BashTool reached session.${String(property)}; nothing may run`);
			},
		},
	);
	const tool = new omp.BashTool(session);
	const { approvalMode, userPolicies } = omp.resolveApprovalFromContext({ settings });
	const restrictions = omp.approvalRestrictions(settings, options.cwd);
	const summary: OmpSettingsSummary = {
		approvalMode,
		userPolicies: Object.keys(userPolicies),
		gitGuard: omp.cfgBashGitGuard.get(settings) === true,
		allowCompoundCommands: omp.cfgBashAllowCompoundCommands.get(settings) === true,
		patterns: omp.cfgBashPatterns.get(settings).length,
		restrictedFolders: restrictions !== undefined,
	};
	const judge = (command: string): Approval => {
		const args = { command };
		const decision = tool.approval(args);
		const critical = typeof decision !== "string" && decision.reason === "Critical pattern detected";
		const resolved = omp.resolveApproval(tool, args, approvalMode, userPolicies, undefined, restrictions);
		const pattern = resolved.reason?.match(PATTERN_REASON)?.[1] ?? null;
		let mechanism: string;
		if (pattern !== null) mechanism = "guard-pattern";
		else if (resolved.source === "user") mechanism = "user-policy";
		else if (resolved.source === "restriction") mechanism = "restricted-folder";
		else if (resolved.source === "mode") mechanism = "mode-default";
		else mechanism = critical ? "critical-pattern" : "tool";
		return { approval: resolved.policy, mechanism, match: pattern, critical };
	};
	return { settings, summary, judge };
}

/** The oracle's JSON-lines protocol: what every answer line starts with. */
const ORACLE_MARK = "\u0001ORACLE ";

type Request = Record<string, unknown>;

let omp: Omp | undefined;
let judge: ((command: string) => Approval) | undefined;
let executeBash: ((command: string, options: Record<string, unknown>) => Promise<Record<string, unknown>>) | undefined;

const text = (value: unknown): string => (typeof value === "string" ? value : "");

function loaded(): Omp {
	if (omp === undefined) throw new Error("the oracle has no omp loaded: send load first");
	return omp;
}

async function load(request: Request): Promise<unknown> {
	const dir = typeof request.omp === "string" ? resolve(request.omp) : findOmpDir();
	if (dir === undefined) throw new OracleUsageError("omp's package not found; pass --omp <package dir>");
	omp = await loadOmp(dir, request.anyVersion === true);
	return {
		dir: omp.dir,
		version: omp.version,
		clientTerminalRoute: omp.clientTerminalRoute,
		permissionKinds: omp.acpPermissionOptions.map((option) => option.kind),
	};
}

async function judgeInit(request: Request): Promise<unknown> {
	const current = loaded();
	const configFiles = Array.isArray(request.configFiles) ? request.configFiles.map(String) : [];
	const agentDir = typeof request.agentDir === "string" ? request.agentDir : undefined;
	const made = await ompApprovals(current, { configFiles, cwd: text(request.cwd), agentDir });
	judge = made.judge;
	return {
		summary: made.summary,
		state: {
			approvalModeConfigured: current.cfgToolsApprovalMode.isConfigured(made.settings),
			interceptorEnabled: current.cfgBashInterceptorEnabled.get(made.settings) === true,
			launchEnabled: current.cfgLaunchEnabled.get(made.settings) !== false,
			asyncEnabled: current.cfgAsyncEnabled.get(made.settings) !== false,
		},
	};
}

async function execInit(request: Request): Promise<unknown> {
	const current = loaded();
	const temp = text(request.temp);
	// The shell sessions omp starts inherit this process's env; the runs also pass it explicitly.
	Object.assign(process.env, request.env as Record<string, string>);
	delete process.env.PI_CONFIG_FILES;
	const load = (path: string) => import(pathToFileURL(join(current.dir, "src", path)).href);
	const { Settings } = await load("config/settings.ts");
	await Settings.init({ cwd: join(temp, "cwd"), agentDir: join(temp, "agent") });
	executeBash = (await load("exec/bash-executor.ts")).executeBash;
	return null;
}

async function exec(request: Request): Promise<unknown> {
	if (executeBash === undefined) throw new Error("the oracle has no shell: send exec-init first");
	const result = await executeBash(text(request.command), {
		cwd: request.cwd,
		timeout: request.timeout,
		gitGuard: request.gitGuard,
		env: request.env,
	});
	return {
		exitCode: result.exitCode ?? null,
		timedOut: result.timedOut === true,
		output: String(result.output ?? ""),
	};
}

async function answer(request: Request): Promise<unknown> {
	switch (request.op) {
		case "find":
			return findOmpDir() ?? null;
		case "load":
			return await load(request);
		case "tokenize":
			return loaded().tokenizeShellSegments(text(request.command));
		case "judge-init":
			return await judgeInit(request);
		case "judge":
			if (judge === undefined) throw new Error("the oracle has no judge: send judge-init first");
			return judge(text(request.command));
		case "exec-init":
			return await execInit(request);
		case "exec":
			return await exec(request);
		default:
			throw new Error(`the oracle doesn't know ${String(request.op)}`);
	}
}

if (import.meta.main && process.argv.length <= 2) {
	for await (const line of createInterface({ input: process.stdin })) {
		if (line.trim() === "") continue;
		let reply: unknown;
		try {
			reply = { ok: await answer(JSON.parse(line) as Request) };
		} catch (error) {
			reply = {
				error: error instanceof Error ? error.message : String(error),
				usage: error instanceof OracleUsageError,
			};
		}
		process.stdout.write(`${ORACLE_MARK}${JSON.stringify(reply)}\n`);
	}
}
