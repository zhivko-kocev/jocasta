/**
 * The guard's rules (`config/guard.yml`, 0450, 0485) and a matcher for them: Jocasta checks the commands it runs
 * itself (a thread's `done_when`, 0580) with it, and H1 and the guard's tests score it (0501). It approximates omp's
 * own `bash.patterns` matching: whitespace runs collapse, `*` is the only wildcard, anchored and case-sensitive; a rule
 * fires on the whole command or on any one segment of its raw text split at `&&`, `||`, `;` and `|`. omp splits with
 * its quote-aware tokenizer instead (quotes removed; also `&`, subshells and newlines) and adds its git guard and
 * critical patterns (0501); H1's `omp-verdicts.ts` asks omp itself.
 */

export type Verdict = "deny" | "prompt" | "allow";

export interface GuardRule {
	match: string;
	approval: "deny" | "prompt";
}

/** Reads a guard file's `bash.patterns` (omp's list of `{match, approval}`); a malformed file throws naming `file`. */
export function parseGuard(text: string, file = "guard"): GuardRule[] {
	const parsed = Bun.YAML.parse(text) as { bash?: { patterns?: unknown } } | null;
	const patterns = parsed?.bash?.patterns;
	if (!Array.isArray(patterns)) throw new Error(`${file}: no bash.patterns list`);
	return patterns.map((rule, i) => {
		const { match, approval } = (rule ?? {}) as { match?: unknown; approval?: unknown };
		if (typeof match !== "string" || (approval !== "deny" && approval !== "prompt"))
			throw new Error(`${file}: bash.patterns[${i}] needs a string match and approval deny or prompt`);
		return { match, approval };
	});
}

const normalize = (text: string) => text.trim().replace(/\s+/g, " ");

/** Whether `pattern` matches `command` as a whole or one of its segments. */
export function matches(command: string, pattern: string): boolean {
	const regex = new RegExp(
		`^${normalize(pattern)
			.split("*")
			.map((part) => part.replace(/[\\^$+?.()|[\]{}]/g, "\\$&"))
			.join(".*")}$`,
	);
	return (
		regex.test(normalize(command)) || command.split(/&&|\|\||;|\|/).some((segment) => regex.test(normalize(segment)))
	);
}

/** A command's verdict: the first matching rule wins; no match allows. */
export function verdict(command: string, rules: readonly GuardRule[]): { verdict: Verdict; rule: string | null } {
	const rule = rules.find((candidate) => matches(command, candidate.match));
	return { verdict: rule?.approval ?? "allow", rule: rule?.match ?? null };
}
