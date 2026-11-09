// H1 (0482, 0500, 0501, 0535): omp 18.18.2's own verdict on every corpus line, computed in-process from omp's installed
// sources, with nothing executed.
//
//   bun bench/h1-guard/omp-verdicts.ts --corpus <corpus.jsonl> --guard <guard.yml> --out <dir> [--omp <package dir>]
//     [--any-version] [--dry-run]
//
// How omp decides before a pane's bash tool runs a command (omp 18.18.2, `@oh-my-pi/pi-coding-agent/src`):
// - `extensibility/extensions/wrapper.ts:269-291,355-374` wraps every tool: `resolveApprovalFromContext`
//   (`tools/approval.ts:97-113`) takes the mode from `tools.approvalMode` (default `yolo`, `tools/settings.ts:314-318`;
//   Jocasta's guard and overlay set none) and `resolveApproval` decides, once on the call as made and again on the
//   input an extension may have revised; deny throws before the tool runs, prompt asks (`:375-506`; with no UI, as in
//   `omp -p`, it throws instead, `:448-471`); then the tool executes (`:508-521`).
// - `tools/approval.ts:366-514`: the tool's own decision first; a tool `deny` wins; under `yolo` a tool `policy` holds
//   and everything else is allowed (so a decision with only `override`, omp's critical patterns, asks nothing);
//   under other modes an override asks and the mode's tier decides the rest; a `tools.restrictedFolders` match then
//   turns anything but a deny into a prompt.
// - `tools/bash.ts:621-681` (`BashTool.approval`): the bash interceptor first (`bashInterceptor`, off by default and in
//   Jocasta's guard; it acts only for tools in the call's `toolNames`) refuses a call it intercepts; then
//   `bash.patterns` (`:257-397`: a line without shell control takes the first rule in list order that matches the
//   whole normalized command; on a line with shell control allow rules never apply, and a deny or prompt rule matches
//   the whole line or any segment from `tools/shell-tokenize.ts:14` `tokenizeShellSegments`, which honours and
//   removes quotes and splits at `; && || | & ( )` and newlines, any deny winning over any prompt); a deny from the
//   operator's layers holds whatever a project layer says (`grantAuthority`); then `CRITICAL_BASH_PATTERNS`
//   (`:197-244`) as an override without a policy, checked before prompt rules; then prompt rules.
//   `bash.allowCompoundCommands` (default off) switches, under a POSIX shell, to per-`&&` segments.
// - `bash.gitGuard` isn't an approval: the embedded shell's `git` builtin refuses at run time
//   (`exec/bash-executor.ts:737-739`).
//
// The mechanism here: it imports omp's `config/settings.ts`, `tools/bash.ts` and `tools/approval.ts` from the
// install, loads the guard as `--config` over omp's defaults (an empty agent dir and cwd, so no user or project
// config), builds the real `BashTool` on a session stub that throws on anything but `settings`, and calls omp's
// `resolveApproval` as the wrapper does, without a tool context: the bash tool's decision reads the context only for
// `toolNames`, which only the interceptor uses, and the guard leaves the interceptor off. `execute` is never called.
// The git guard has no check that doesn't run the command: with `--git-guard <git-guard-check.jsonl>` the refusals
// measured in the VM (git-guard-check.ts ran those lines through omp's embedded shell) decide for the lines it
// covers, and `git-guard.ts`'s reimplementation, applied on omp's own tokenizer's argv, for the rest; each line names
// its source (`gitGuardSource`). A line omp's approval allows and the git guard refuses is `deny` by `git-guard`; a
// line omp asks about stays `prompt`.
//
// Writes `omp-verdicts.jsonl` (id, command, expected, variant, base, category, verdict, mechanism, match, approval,
// critical, gitGuard, gitGuardSource, reimplemented = evaluate.ts's verdict) and `omp-verdicts.json` (version,
// settings, counts), and prints where omp and the reimplementation disagree, by expected class and by way of writing.
import { existsSync, mkdtempSync, readFileSync, realpathSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { removeTree } from "../../src/shared/fsx.ts";
import { verdict } from "../../src/shared/guard.ts";
import { BenchUsageError, flag, parseBenchArgs, REPO_ROOT, runScript } from "../lib/args.ts";
import { appendRow, conditions, printPlan, writeJson } from "../lib/output.ts";
import { type CorpusEntry, parseGuard, readCorpus, VARIANTS, type Verdict } from "./evaluate.ts";
import { gitGuardRefusalOf, type MeasuredGitGuardLine, parseMeasuredGitGuard } from "./git-guard.ts";

export const PINNED_OMP = "18.18.2";
const PACKAGE = "@oh-my-pi/pi-coding-agent";

type Decision = string | { tier: string; reason?: string; override?: boolean; policy?: Verdict };
interface Resolved {
	policy: Verdict;
	source?: "tool" | "user" | "mode" | "restriction";
	reason?: string;
}
interface ApprovalSubject {
	name: string;
	approval: (args: unknown, context?: { toolNames?: string[] }) => Decision;
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
	if (version === undefined) throw new BenchUsageError(`${dir} is not omp's package (${PACKAGE})`);
	if (version !== PINNED_OMP && !anyVersion)
		throw new BenchUsageError(`omp ${version} at ${dir}; H1 runs on ${PINNED_OMP} (0535), or pass --any-version`);
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

export interface OmpVerdict {
	verdict: Verdict;
	/** guard-pattern, git-guard, critical-pattern, mode-default, user-policy, restricted-folder or tool. */
	mechanism: string;
	/** The guard pattern, or the git guard's refusal. */
	match: string | null;
	/** omp's approval alone (before the git guard). */
	approval: Verdict;
	/** omp's `CRITICAL_BASH_PATTERNS` flagged the line (under `yolo` that asks nothing). */
	critical: boolean;
	/** The git guard's refusal, whatever the approval said (from `gitGuardSource`). */
	gitGuard: string | null;
	/**
	 * Where `gitGuard` came from: `measured` (git-guard-check.ts ran the line through omp's embedded shell),
	 * `reimplemented` (git-guard.ts), or `off` (the guard has no `bash.gitGuard`).
	 */
	gitGuardSource: "measured" | "reimplemented" | "off";
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

/** A measured git guard outcome for one line: its refusal message, or null when the guard let it run. */
export interface MeasuredGitGuard {
	refusal: string | null;
}

/**
 * Loads settings as a pane would (`configFiles` as `--config`, in order) and returns omp's judge for one command.
 * `agentDir` unset means omp's own (his global config); H1 passes an empty folder so only omp's defaults and the
 * guard apply.
 */
export async function ompJudge(
	omp: Omp,
	options: { configFiles: string[]; cwd: string; agentDir?: string },
): Promise<{
	settings: unknown;
	summary: OmpSettingsSummary;
	judge: (command: string, measured?: MeasuredGitGuard) => OmpVerdict;
}> {
	delete process.env.PI_CONFIG_FILES;
	const settings = await omp.Settings.loadReadOnly(options);
	// The real BashTool; its approval reads only `session.settings`. Anything else means a path towards execution.
	const session = new Proxy(
		{},
		{
			get(_target, property) {
				if (property === "settings") return settings;
				if (property === "then") return undefined;
				throw new Error(`H1: BashTool reached session.${String(property)}; nothing may run`);
			},
		},
	);
	const tool = new omp.BashTool(session);
	const { approvalMode, userPolicies } = omp.resolveApprovalFromContext({ settings });
	const restrictions = omp.approvalRestrictions(settings, options.cwd);
	const gitGuardOn = omp.cfgBashGitGuard.get(settings) === true;
	const summary: OmpSettingsSummary = {
		approvalMode,
		userPolicies: Object.keys(userPolicies),
		gitGuard: gitGuardOn,
		allowCompoundCommands: omp.cfgBashAllowCompoundCommands.get(settings) === true,
		patterns: omp.cfgBashPatterns.get(settings).length,
		restrictedFolders: restrictions !== undefined,
	};
	const judge = (command: string, measured?: MeasuredGitGuard): OmpVerdict => {
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
		const gitGuardSource = !gitGuardOn ? "off" : measured === undefined ? "reimplemented" : "measured";
		let gitGuard: string | null = null;
		if (gitGuardSource === "measured") gitGuard = measured?.refusal ?? null;
		else if (gitGuardSource === "reimplemented") gitGuard = gitGuardRefusalOf(omp.tokenizeShellSegments(command));
		const common = { critical, gitGuard, gitGuardSource } as const;
		if (gitGuard !== null && resolved.policy === "allow")
			return { verdict: "deny", mechanism: "git-guard", match: gitGuard, approval: "allow", ...common };
		return { verdict: resolved.policy, mechanism, match: pattern, approval: resolved.policy, ...common };
	};
	return { settings, summary, judge };
}

export interface VerdictLine extends OmpVerdict {
	id: string;
	command: string;
	expected: Verdict;
	variant: string;
	base: string;
	category: string;
	reimplemented: Verdict;
}

export interface Disagreement {
	key: string;
	n: number;
	disagree: number;
	/** `old→new` (reimplemented→omp) and how many lines moved that way. */
	moves: Record<string, number>;
}

/** Where omp's verdict differs from the reimplementation's, grouped by `keyOf`, in first-seen key order. */
export function disagreements(
	lines: readonly { verdict: Verdict; reimplemented: Verdict }[],
	keyOf: (index: number) => string,
): Disagreement[] {
	const byKey = new Map<string, Disagreement>();
	lines.forEach((line, index) => {
		const key = keyOf(index);
		let entry = byKey.get(key);
		if (entry === undefined) {
			entry = { key, n: 0, disagree: 0, moves: {} };
			byKey.set(key, entry);
		}
		entry.n += 1;
		if (line.verdict === line.reimplemented) return;
		entry.disagree += 1;
		const move = `${line.reimplemented}→${line.verdict}`;
		entry.moves[move] = (entry.moves[move] ?? 0) + 1;
	});
	return [...byKey.values()];
}

const renderDisagreements = (title: string, rows: readonly Disagreement[]) =>
	[
		`| ${title} | lines | disagree | reimplemented→omp |`,
		"|---|---|---|---|",
		...rows.map(
			(row) =>
				`| ${row.key} | ${row.n} | ${row.disagree} | ${
					Object.entries(row.moves)
						.sort((a, b) => b[1] - a[1])
						.map(([move, count]) => `${move} ${count}`)
						.join(", ") || "—"
				} |`,
		),
	].join("\n");

export function judgeCorpus(
	corpus: readonly CorpusEntry[],
	judge: (command: string, measured?: MeasuredGitGuard) => OmpVerdict,
	reimplemented: (command: string) => Verdict,
	measured: ReadonlyMap<string, MeasuredGitGuardLine> = new Map(),
): VerdictLine[] {
	return corpus.map((entry, i) => {
		if (entry.id === undefined) throw new BenchUsageError(`corpus line ${i + 1} has no id`);
		const check = measured.get(entry.id);
		if (check !== undefined && check.command !== entry.command)
			throw new BenchUsageError(`git guard check: ${entry.id}'s command differs from the corpus's`);
		return {
			id: entry.id,
			command: entry.command,
			expected: entry.expected,
			variant: entry.variant ?? "",
			base: entry.base ?? "",
			category: entry.category ?? "",
			...judge(entry.command, check),
			reimplemented: reimplemented(entry.command),
		};
	});
}

if (import.meta.main) {
	if (process.argv.slice(2).some((arg) => arg === "--help" || arg === "-h")) {
		console.log(
			"usage: bun bench/h1-guard/omp-verdicts.ts --corpus <corpus.jsonl> --guard <guard.yml> --out <dir> [--git-guard <git-guard-check.jsonl>] [--omp <package dir>] [--any-version] [--dry-run]",
		);
		process.exit(0);
	}
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 1,
			valued: ["--corpus", "--guard", "--omp", "--git-guard"],
			booleans: ["--any-version"],
		});
		const corpusPath = flag(args, "corpus");
		if (corpusPath === undefined) throw new BenchUsageError("--corpus <path.jsonl> is required");
		const guardPath = resolve(flag(args, "guard", resolve(REPO_ROOT, "config", "guard.yml")));
		const gitGuardPath = flag(args, "git-guard");
		for (const path of [corpusPath, guardPath, ...(gitGuardPath === undefined ? [] : [gitGuardPath])])
			if (!existsSync(path)) throw new BenchUsageError(`${path} does not exist`);
		const measured =
			gitGuardPath === undefined
				? new Map<string, MeasuredGitGuardLine>()
				: parseMeasuredGitGuard(readFileSync(gitGuardPath, "utf8"), gitGuardPath);
		const ompText = flag(args, "omp");
		const ompDir = ompText === undefined ? findOmpDir() : resolve(ompText);
		if (ompDir === undefined) throw new BenchUsageError(`omp's package not found; pass --omp <dir of ${PACKAGE}>`);
		const { entries: corpus, sha256: corpusSha256 } = readCorpus(corpusPath);
		const rules = parseGuard(readFileSync(guardPath, "utf8"), guardPath);
		if (args.dryRun) {
			printPlan(
				"H1 omp verdicts",
				[
					`import omp from ${ompDir} (refusing anything but ${PINNED_OMP}${args.flags.has("any-version") ? ", overridden by --any-version" : ""})`,
					`load ${guardPath} as --config over omp's defaults (empty agent dir and cwd); build BashTool on a session stub that throws`,
					`ask omp's resolveApproval for each of ${corpus.length} commands; git guard ${gitGuardPath === undefined ? "reimplemented (git-guard.ts)" : `measured for the ${measured.size} lines in ${gitGuardPath}, reimplemented (git-guard.ts) for the rest`}; nothing executes`,
					"compare with the reimplementation (evaluate.ts) by expected class and by way of writing",
				],
				args.out,
			);
			return;
		}
		const out = args.out;
		if (out === undefined) throw new BenchUsageError("--out <dir> is required (outside the repo)");
		if (existsSync(join(out, "omp-verdicts.jsonl")))
			throw new BenchUsageError(`${join(out, "omp-verdicts.jsonl")} exists; results are never overwritten`);
		const started = conditions([]);
		const omp = await loadOmp(ompDir, args.flags.has("any-version"));
		console.log(`omp ${omp.version} from ${omp.dir}`);
		const empty = mkdtempSync(join(tmpdir(), "h1-omp-"));
		try {
			const { summary, judge } = await ompJudge(omp, { configFiles: [guardPath], cwd: empty, agentDir: empty });
			console.log(`settings: ${JSON.stringify(summary)}`);
			const lines = judgeCorpus(corpus, judge, (command) => verdict(command, rules).verdict, measured);
			for (const line of lines) appendRow(out, "omp-verdicts", line);
			const byExpected = disagreements(lines, (i) => lines[i]?.expected ?? "");
			const variantOrder: readonly string[] = VARIANTS;
			const byVariant = disagreements(lines, (i) => lines[i]?.variant ?? "").sort(
				(a, b) => variantOrder.indexOf(a.key) - variantOrder.indexOf(b.key),
			);
			const mechanisms: Record<string, number> = {};
			for (const line of lines) mechanisms[line.mechanism] = (mechanisms[line.mechanism] ?? 0) + 1;
			const gitGuardSources: Record<string, number> = {};
			for (const line of lines) gitGuardSources[line.gitGuardSource] = (gitGuardSources[line.gitGuardSource] ?? 0) + 1;
			console.log(`git guard sources: ${JSON.stringify(gitGuardSources)}`);
			console.log(renderDisagreements("Expected", byExpected));
			console.log("");
			console.log(renderDisagreements("Way of writing", byVariant));
			console.log(`mechanisms: ${JSON.stringify(mechanisms)}`);
			const file = writeJson(out, "omp-verdicts", {
				benchmark: "H1",
				conditions: { ...started, finishedAt: new Date().toISOString(), versions: { omp: omp.version } },
				omp: { version: omp.version, sources: "src/ of the installed package" },
				guard: guardPath,
				corpusSha256,
				settings: summary,
				gitGuardCheck: gitGuardPath === undefined ? null : resolve(gitGuardPath),
				gitGuardSources,
				reimplemented: gitGuardSources.reimplemented
					? ["git guard (git-guard.ts) where gitGuardSource is reimplemented"]
					: [],
				commands: lines.length,
				mechanisms,
				disagreements: { byExpected, byVariant },
			});
			console.log(`wrote ${file}`);
		} finally {
			removeTree(empty);
		}
	});
}
