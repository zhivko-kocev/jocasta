/**
 * hunk (0394, 0406): the diff pane that opens beside a thread when it finishes a task, the review prompt the thread
 * gets, and the count of its notes. hunk ships as its npm platform binary (`hunkdiff-<platform>@0.23.0`).
 *
 * hunk's live sessions go through one session daemon per loopback port (`hunk daemon serve`, default port 47657),
 * which the first hunk window starts detached in its own working folder and which outlives it (hunk
 * `src/session/broker/brokerLauncher.ts:688-708`). Jocasta's hunk uses its own port (`HUNK_PORT`), so another hunk
 * copy on the machine (another version is refused by the daemon) never breaks the note count (F-03), and Jocasta starts
 * that daemon itself from the home folder before any hunk pane opens, so no daemon sits in a worktree and blocks its
 * removal on Windows (F-02).
 */
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { gitAsync } from "../../shared/git.ts";
import { installDir } from "../../shared/install.ts";
import { waitFor } from "../../shared/poll.ts";
import { runProgram } from "../../shared/process.ts";
import { REVIEW_LEAD } from "./wording.ts";

/** The loopback port of Jocasta's hunk session daemon (`HUNK_MCP_PORT`; hunk's own default is 47657). */
export const HUNK_PORT = 47658;

/**
 * The environment of every hunk Jocasta starts, and of thread panes (their review runs hunk's session commands): the
 * daemon's port, and no update notice (bumps are deliberate, 0383; hunk `src/core/process/updateNotice.ts:17`).
 */
export function hunkEnv(port = HUNK_PORT): Record<string, string> {
	return { HUNK_MCP_PORT: String(port), HUNK_DISABLE_UPDATE_NOTICE: "1" };
}

/** `hunk diff`'s argv for a thread's review pane: Tokyo Night like herdr and omp (0401, UX-08), watching the range. */
export function hunkDiffArgs(hunk: string[], range: string): string[] {
	return [...hunk, "diff", "--theme", "tokyo-night", range, "--watch"];
}

/** The npm package holding hunk's binary for this platform, or undefined where hunk ships none. */
export function hunkPackage(platform = process.platform, arch = process.arch): string | undefined {
	if (platform === "win32" && arch === "x64") return "hunkdiff-windows-x64";
	if (platform === "linux" && arch === "x64") return "hunkdiff-linux-x64";
	if (platform === "darwin" && arch === "arm64") return "hunkdiff-darwin-arm64";
	return undefined;
}

/** The package's binary from its `package.json` `bin.hunk` (`bin/hunk.exe` on Windows, `bin/hunk` elsewhere). */
function packageBinary(packageDir: string): string | undefined {
	try {
		const manifest = JSON.parse(readFileSync(join(packageDir, "package.json"), "utf8")) as {
			bin?: string | Record<string, string>;
		};
		const bin = typeof manifest.bin === "string" ? manifest.bin : manifest.bin?.hunk;
		return bin ? join(packageDir, bin) : undefined;
	} catch {
		return undefined;
	}
}

/**
 * hunk's binary: `<install dir>/hunk/hunk[.exe]` beside `jocasta.exe` (the service finds its install the same way),
 * else `node_modules/hunkdiff-<platform>`'s in a source run; undefined when none is there.
 */
export function findHunk(): string | undefined {
	const install = installDir();
	if (install) {
		const shipped = join(install, "hunk", process.platform === "win32" ? "hunk.exe" : "hunk");
		if (existsSync(shipped)) return shipped;
	}
	const pkg = hunkPackage();
	if (!pkg) return undefined;
	const binary = packageBinary(join(import.meta.dir, "..", "..", "..", "node_modules", pkg));
	return binary && existsSync(binary) ? binary : undefined;
}

/** What the review turn hands omp's reviewer, and the checks Jocasta ran for it (0453, 0459, 0473, 0539, 0540, 0546). */
export interface ReviewContext {
	/** The approved plan's file, when the thread's session has one. */
	plan?: string;
	/** The project's rules files that exist: the worktree's `AGENTS.md`, the project's `AGENTS.local.md` (0449). */
	rules: string[];
	/**
	 * The gate reply of the checks Jocasta's runner ran on the task's tree (the project's mise check and each
	 * `done_when`, 0542, 0546): "" when every gate passed or none ran.
	 */
	gates?: string;
	/** Jocasta's runner ran the project's own check; otherwise the turn runs the project's tests itself. */
	checked?: boolean;
	/** The thread's brief from its task file (0540), when recorded. */
	brief?: string;
	/** The thread's `report_result` as one line (0539); null when it handed in none; undefined without a task file. */
	report?: string | null;
}

/**
 * What the thread is asked once its hunk pane is open (0394, 0425, 0453, 0460, 0546). Jocasta's runner already ran
 * the project's checks (`gates`): their failures are given as gates, a pass is not mentioned; a project without known
 * checks has the turn run its tests as one bash command and end on a `TESTS: pass|fail — <command>` (or `TESTS:
 * none`) line the service checks against the session's bash results (`sessions.ts` `checkedTests`). Then its own
 * `verify` skill when it has one, then omp's reviewer subagent on the range with the approved plan, the project's
 * rules and its lessons, told to report only material findings; every finding goes into the live hunk session with
 * one `comment apply` batch, each summary led by its priority (`[P0]`…`[P3]`). The shipped platform binary doesn't
 * carry hunk's `hunk-review` skill (`hunk skill path` fails there), so the commands and the batch shape are spelled
 * out here. hunk's argv becomes one command line for the thread's bash tool: odd words double-quoted, `\` as `/`.
 */
export function reviewPrompt(hunk: string[], range: string, context: ReviewContext = { rules: [] }): string {
	const cli = hunk
		.map((word) => (/^[\w./:=-]+$/.test(word) ? word : `"${word.replace(/\\/g, "/").replace(/"/g, '\\"')}"`))
		.join(" ");
	const slash = (path: string) => path.replace(/\\/g, "/");
	const flat = (text: string) => text.replace(/\s+/g, " ").replace(/[«»]/g, '"').trim();
	const given = [
		...(context.brief ? [`his brief «${flat(context.brief)}», to check the diff against what he asked for`] : []),
		...(context.plan ? [`the approved plan (${slash(context.plan)}), to check the diff against it`] : []),
		...(context.rules.length > 0 ? [`the project's rules (${context.rules.map(slash).join(", ")})`] : []),
		"its lessons in skill://lessons (a change that contradicts a lesson is a finding citing it)",
		...(context.report === null
			? ["the fact that the thread handed in no report"]
			: context.report
				? [`the thread's own report «${flat(context.report)}» (its claims, not proof)`]
				: []),
	];
	const ranTests = context.checked !== true;
	const failed = context.gates
		? `Jocasta ran the task's checks on this tree and they failed; give the reviewer these gates too:\n\n${context.gates}\n\n`
		: undefined;
	return [
		REVIEW_LEAD,
		...(failed ? [failed] : []),
		ranTests
			? "First run the project's tests the way its AGENTS.md or package scripts say, as one bash command;"
			: undefined,
		ranTests
			? "when the project has a `verify` skill, read skill://verify and run it too. Don't fix failures in this turn."
			: "When the project has a `verify` skill, read skill://verify and run it. Don't fix failures in this turn.",
		`Then run the reviewer subagent on \`git diff ${range}\` in this worktree. In its task give it ${given.join(", ")},`,
		"and tell it: report only material findings; prefer one strong finding over several weak ones.",
		`A hunk window beside you shows the same diff. Post every finding in it as one batch: pipe`,
		'`{"comments":[{"filePath":"<path>","newLine":<n>,"summary":"[P<priority>] <one sentence>","rationale":"<why>","author":"reviewer"}]}`',
		`into \`${cli} session comment apply --repo . --stdin\` (each summary starts with the reviewer's priority, [P0] to [P3];`,
		`use "oldLine" for removed lines; check the files and hunks first with \`${cli} session review --repo . --json\`).`,
		`Don't run \`${cli} diff\` yourself, and don't change code in this turn. No findings: post nothing and say so.`,
		...(ranTests
			? [
					"End your reply with one line on its own: `TESTS: pass — <the test command you ran>`,",
					"`TESTS: fail — <the test command you ran>`, or `TESTS: none` when the project has no tests to run.",
				]
			: []),
	]
		.filter((part) => part !== undefined)
		.join(" ");
}

/** A review's notes in hunk: how many, and how many the reviewer marked blocking (P0 or P1, 0453). */
export interface ReviewNotes {
	notes: number;
	blocking: number;
}

/**
 * The notes in `hunk session comment list --json`'s output (`{comments: [{createdAt, summary, ...}]}`, hunk
 * `src/session/types.ts:176-183`) made at `since` (epoch ms) or later: the live session keeps earlier reviews' notes
 * (`--watch` keeps the pane across tasks). A note without a time counts; one whose summary starts with `[P0]` or
 * `[P1]` is blocking.
 */
export function countComments(json: string, since = 0): ReviewNotes {
	const parsed = JSON.parse(json) as { comments?: unknown };
	if (!Array.isArray(parsed.comments)) throw new Error("hunk's comment list has no comments array");
	const recent = parsed.comments.filter((comment: { createdAt?: unknown }) => {
		const at = typeof comment?.createdAt === "string" ? Date.parse(comment.createdAt) : Number.NaN;
		return Number.isNaN(at) || at >= since;
	}) as { summary?: unknown }[];
	const blocking = recent.filter(
		(comment) => typeof comment?.summary === "string" && /^\s*\[?P[01]\b/i.test(comment.summary),
	).length;
	return { notes: recent.length, blocking };
}

/** How long a hunk command may take before it counts as failed. */
const HUNK_COMMAND_MS = 20_000;

/** Runs hunk from the home folder (never inside a worktree) with Jocasta's hunk environment; stdout, or throws. */
async function runHunk(hunk: string[], args: string[], port: number): Promise<string> {
	const run = await runProgram([...hunk, ...args], {
		cwd: homedir(),
		env: { ...process.env, ...hunkEnv(port) },
		timeoutMs: HUNK_COMMAND_MS,
	});
	const what = `hunk ${args.slice(0, 3).join(" ")}`;
	if (run.timedOut || run.signal) throw new Error(`${what} took over ${HUNK_COMMAND_MS / 1000} s`);
	if (run.code !== 0) throw new Error(`${what} failed (${run.code}): ${run.stderr.trim() || run.stdout.trim()}`);
	return run.stdout;
}

/** Counts the worktree's live notes made since `since` (`hunk session comment list --repo <worktree> --json`). */
export async function listComments(
	hunk: string[],
	worktree: string,
	since = 0,
	port = HUNK_PORT,
): Promise<ReviewNotes> {
	return countComments(await runHunk(hunk, ["session", "comment", "list", "--repo", worktree, "--json"], port), since);
}

/** What a reviewed diff did (0453): its size, the test files it changed, the test files it added a skip/only to. */
export interface ReviewProof {
	added: number;
	deleted: number;
	files: number;
	testFiles: string[];
	skipped: string[];
}

/** A test file by its path: under a `test`/`tests`/`__tests__`/`spec` folder, or named like one. */
const TEST_FILE =
	/(^|\/)(tests?|__tests__|specs?)\/|[._-](test|spec)\.[^/]+$|(^|\/)test_[^/]+\.py$|_test\.(go|py|rb|exs?)$/i;

/** An added line that skips or focuses tests: `.skip(`/`.only(`, `xit(`, `fit(`, pytest/unittest skips, `#[ignore]`, `t.Skip(`. */
const SKIP_MARK =
	/\b(it|test|describe|context|suite)\.(skip|only|todo)\s*\(|\b(xit|xdescribe|xtest|fit|fdescribe)\s*\(|@pytest\.mark\.(skip|xfail)|@unittest\.skip|#\[ignore\]|\bt\.Skip(Now|f)?\s*\(/;

/** One git call of the proof (CO-10): 30 s at most, off the event loop. */
const PROOF_GIT_MS = 30_000;

/** Test files per `git diff` call, well inside Windows' 32K command line. */
const PROOF_FILES_PER_CALL = 100;

/**
 * The proof line's facts for `range` (`<trunk>...<branch>`) in `cwd`, from git without holding the event loop: one
 * `--numstat` for the size and the files, and one `-U0` diff of the test files (split at its `diff --git` headers)
 * for added skip/only markers. Undefined when git can't tell.
 */
export async function reviewProof(cwd: string, range: string): Promise<ReviewProof | undefined> {
	// Paths as they are (no C-quoting of non-ASCII), so the test files' names match and split back out.
	const unquoted = ["-c", "core.quotePath=false"];
	const numstat = await gitAsync(cwd, [...unquoted, "diff", "--numstat", "--no-renames", range], PROOF_GIT_MS);
	if (numstat.code !== 0) return undefined;
	let added = 0;
	let deleted = 0;
	const paths: string[] = [];
	for (const line of numstat.stdout.split("\n").filter(Boolean)) {
		const [plus = "0", minus = "0", ...path] = line.split("\t");
		added += Number(plus) || 0;
		deleted += Number(minus) || 0;
		paths.push(path.join("\t"));
	}
	const testFiles = paths.filter((path) => TEST_FILE.test(path));
	const marked = new Set<string>();
	for (let at = 0; at < testFiles.length; at += PROOF_FILES_PER_CALL) {
		const files = testFiles.slice(at, at + PROOF_FILES_PER_CALL);
		const args = [...unquoted, "diff", "-U0", "--no-color", "--no-renames", range, "--", ...files];
		const diff = await gitAsync(cwd, args, PROOF_GIT_MS);
		for (const part of diff.stdout.split(/^diff --git /m)) {
			const file = /^\+\+\+ b\/(.+)$/m.exec(part)?.[1];
			if (!file) continue;
			const skips = part
				.split("\n")
				.some((line) => line.startsWith("+") && !line.startsWith("+++") && SKIP_MARK.test(line));
			if (skips) marked.add(file);
		}
	}
	const skipped = testFiles.filter((file) => marked.has(file));
	return { added, deleted, files: paths.length, testFiles, skipped };
}

/** The proof as the review event says it: `+12/−3 in 2 files, test files changed: a.test.ts, adds skip/only in a.test.ts`. */
export function proofSaid(proof: ReviewProof): string {
	const parts = [`+${proof.added}/−${proof.deleted} in ${proof.files} file${proof.files === 1 ? "" : "s"}`];
	if (proof.testFiles.length > 0) parts.push(`test files changed: ${proof.testFiles.join(", ")}`);
	else parts.push("no test files changed");
	if (proof.skipped.length > 0) parts.push(`adds skip/only in ${proof.skipped.join(", ")}`);
	return parts.join(", ");
}

/** Whether Jocasta's hunk daemon answers its health check (hunk `brokerLauncher.ts:588`). */
async function daemonUp(port: number): Promise<boolean> {
	try {
		const response = await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(1_000) });
		await response.body?.cancel();
		return response.ok;
	} catch {
		return false;
	}
}

/**
 * Jocasta's hunk session daemon, started detached from the home folder when it isn't up (F-02), waiting up to 5 s for
 * it. hunk's own lock and health check keep a second daemon from starting on the same port. `started` gets the pid of
 * a daemon started here, so Jocasta's close can end it (0434, 0486).
 */
export async function ensureHunkDaemon(
	hunk: string[],
	port = HUNK_PORT,
	started?: (pid: number) => void,
): Promise<void> {
	if (await daemonUp(port)) return;
	const [command, ...args] = hunk;
	if (!command) return;
	const child = spawn(command, [...args, "daemon", "serve"], {
		cwd: homedir(),
		env: { ...process.env, ...hunkEnv(port) },
		detached: true,
		stdio: "ignore",
		windowsHide: true,
	});
	child.unref();
	if (child.pid !== undefined) started?.(child.pid);
	// The first look after 100 ms: the daemon was just started.
	await Bun.sleep(100);
	if (await waitFor(() => daemonUp(port), { timeoutMs: 4_900, intervalMs: 100 })) return;
	throw new Error(`hunk's session daemon didn't start on 127.0.0.1:${port}`);
}
