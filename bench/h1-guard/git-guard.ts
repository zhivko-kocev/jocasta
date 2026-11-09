// omp's `bash.gitGuard` as H1 reads it: a REIMPLEMENTATION, the one part of omp-verdicts.ts that isn't omp's own code,
// until git-guard-check.ts has measured the real guard in the VM (its lines then replace this for the lines it ran).
//
// omp 18.18.2 enforces the git guard inside its embedded shell, not in the bash tool's approval: with
// `bash.gitGuard: true` the tool passes `PI_GIT_GUARD=1` to the native shell (`exec/bash-executor.ts:737-739`) and that
// shell's `git` builtin refuses the command when it runs (`exec/settings.ts:143-154`). The builtin lives in
// pi-natives' Rust binary and exports no check that doesn't run the command, so H1 reads its rules from the setting's
// description and the refusal messages in `pi_natives.win32-x64-baseline.node`. It first tells whose checkout git is
// in. In a checkout of the session's own (a linked worktree from `git worktree add`, or an ovnfs checkout) the forms
// below run after the guard saves the uncommitted changes they would discard, refused when saving fails or the
// command recurses into submodules, and stash forms that could touch another session's entry stay refused (every
// worktree shares one stash list) [inference from the messages]. In a repository's main checkout, which other
// sessions may share, it refuses:
// - `git stash`, except `stash list`, `stash show` and `stash create`;
// - `git reset --hard`, and `git reset` to another commit (`HEAD~1`, a sha, a branch); bare `git reset` and
//   `git reset [--] <path>` (unstaging) run;
// - `git checkout`/`git switch` to another branch or commit, always; `git checkout -- <path>` and `git restore <path>`
//   outside a merge or rebase conflict (H1's pane is in none); `--force`/`--discard-changes` and whole-tree pathspecs
//   always; `git restore --staged <path>` (unstaging) runs;
// - any of those when `--git-dir`, `--work-tree`, `GIT_DIR`, `GIT_WORK_TREE` or `GIT_INDEX_FILE` points git elsewhere.
// This reimplements the main-checkout rules, as H1's lines run in one (git-guard-check.ts's fixture at C:\work\demo).
// Only a word that is exactly `git` reaches the builtin [inference]: `git.exe`, a full path to git, and git inside
// `pwsh -c`/`cmd /c` start the external binary, which the embedded shell doesn't guard. Telling a commit from a path
// in `git reset <x>` needs the repository; here only revision-shaped words count as commits (H1's corpus resets all
// carry `--hard`, so this never decides a corpus line).
import { BenchUsageError } from "../lib/args.ts";
import { strictJsonLines } from "../lib/jsonl.ts";

const GUARDED: Record<string, true> = { stash: true, reset: true, checkout: true, switch: true, restore: true };
const REDIRECTING_ENV = /^(GIT_DIR|GIT_WORK_TREE|GIT_INDEX_FILE)=/;
const ASSIGNMENT = /^[A-Za-z_][A-Za-z0-9_]*=/;
const GLOBAL_WITH_VALUE: Record<string, true> = {
	"-C": true,
	"-c": true,
	"--git-dir": true,
	"--work-tree": true,
	"--namespace": true,
	"--config-env": true,
	"--exec-path": true,
};
const REVISION =
	/^(?:(?:HEAD|ORIG_HEAD|FETCH_HEAD|MERGE_HEAD|@)(?:[~^]\d*|@\{[^}]*\})+|[0-9a-f]{7,40}|(?:origin|upstream)\/\S+)$/;

/** Why the git guard refuses this argv, or null when it lets it run (or it isn't the `git` builtin). */
export function gitGuardRefusal(argv: readonly string[]): string | null {
	let i = 0;
	let redirected = false;
	while (i < argv.length && ASSIGNMENT.test(argv[i] ?? "")) {
		if (REDIRECTING_ENV.test(argv[i] ?? "")) redirected = true;
		i++;
	}
	if (argv[i] !== "git") return null;
	i++;
	for (; i < argv.length; i++) {
		const word = argv[i] ?? "";
		if (!word.startsWith("-")) break;
		const name = word.split("=")[0] ?? word;
		if (name === "--git-dir" || name === "--work-tree") redirected = true;
		if (Object.hasOwn(GLOBAL_WITH_VALUE, word)) i++;
	}
	const sub = argv[i];
	if (sub === undefined || !Object.hasOwn(GUARDED, sub)) return null;
	const rest = argv.slice(i + 1);
	const refusal = subcommandRefusal(sub, rest);
	if (refusal === null) return null;
	return redirected ? `git ${sub} with git pointed elsewhere` : refusal;
}

function subcommandRefusal(sub: string, rest: readonly string[]): string | null {
	const dashDash = rest.indexOf("--");
	const before = dashDash === -1 ? rest : rest.slice(0, dashDash);
	const positionals = before.filter((word) => !word.startsWith("-"));
	switch (sub) {
		case "stash":
			return ["list", "show", "create"].includes(rest[0] ?? "") ? null : "git stash";
		case "reset": {
			if (rest.includes("--hard")) return "git reset --hard";
			return positionals.some((word) => REVISION.test(word)) ? "git reset to another commit" : null;
		}
		case "checkout":
		case "switch":
			return rest.length === 0 ? null : `git ${sub}`;
		case "restore": {
			const staged = rest.includes("--staged") || rest.includes("-S");
			const worktree = rest.includes("--worktree") || rest.includes("-W");
			return rest.length === 0 || (staged && !worktree) ? null : "git restore";
		}
		default:
			return null;
	}
}

/** The first refusal over a command's segments (omp's tokenizer splits them), or null. */
export function gitGuardRefusalOf(segments: readonly (readonly string[])[]): string | null {
	for (const segment of segments) {
		const refusal = gitGuardRefusal(segment);
		if (refusal !== null) return refusal;
	}
	return null;
}

const GIT_WORD = /^(?:git|git\.exe|.*[\\/]git(?:\.exe)?)$/i;
const WRAPPER_FLAG: Record<string, readonly string[]> = {
	pwsh: ["-c", "-command"],
	"pwsh.exe": ["-c", "-command"],
	powershell: ["-c", "-command"],
	"powershell.exe": ["-c", "-command"],
	cmd: ["/c", "/k"],
	"cmd.exe": ["/c", "/k"],
};

/**
 * The first git argv of a line made only of git (any spelling: `git`, `git.exe`, a path to git, `git -C …`, also
 * inside `pwsh -c`/`cmd /c`), `cd` and `echo`, or null for any other line. `tokenize` is omp's tokenizer.
 */
export function gitOnlyArgv(command: string, tokenize: (command: string) => string[][]): string[] | null {
	let git: string[] | null = null;
	const visit = (segments: string[][], nested: boolean): boolean =>
		segments.every((segment) => {
			const first = segment[0] ?? "";
			if (first === "cd" || first === "echo") return true;
			if (GIT_WORD.test(first)) {
				git ??= segment;
				return true;
			}
			const flags = WRAPPER_FLAG[first.toLowerCase()];
			if (nested || flags === undefined) return false;
			const at = segment.findIndex((word, i) => i > 0 && flags.includes(word.toLowerCase()));
			return at !== -1 && visit(tokenize(segment.slice(at + 1).join(" ")), true);
		});
	return visit(tokenize(command), false) ? git : null;
}

/** The git subcommand of a git argv, past its global options (`-C <path>`, `-c <k=v>`, …). */
export function gitSubcommand(argv: readonly string[]): string {
	for (let i = 1; i < argv.length; i++) {
		const word = argv[i] ?? "";
		if (!word.startsWith("-")) return word;
		if (Object.hasOwn(GLOBAL_WITH_VALUE, word)) i++;
	}
	return "";
}

/** One measured line of `git-guard-check.jsonl` as omp-verdicts.ts reads it. */
export interface MeasuredGitGuardLine {
	command: string;
	/** The guard's refusal message, or null when omp's shell ran the line with the guard on. */
	refusal: string | null;
}

/** Reads `git-guard-check.jsonl`: by id, the lines that were measured (a line whose fixture failed is left out). */
export function parseMeasuredGitGuard(text: string, file = "git-guard-check"): Map<string, MeasuredGitGuardLine> {
	const lines = new Map<string, MeasuredGitGuardLine>();
	for (const { row, where } of strictJsonLines(text, file)) {
		const { id, command, measured, refusal } = row;
		if (
			typeof id !== "string" ||
			typeof command !== "string" ||
			typeof measured !== "boolean" ||
			(refusal !== null && typeof refusal !== "string")
		)
			throw new BenchUsageError(`${where}: needs id, command, measured and refusal (string or null)`);
		if (measured) lines.set(id, { command, refusal });
	}
	return lines;
}
