import { BenchUsageError } from "./args.ts";

/**
 * A JSONL input the bench reads strictly (a task list, a guard corpus, verdicts): unlike `src/shared/jsonl.ts`, which
 * skips what doesn't parse, a malformed line here is a usage error naming it. Yields each non-blank line parsed, in
 * order, with `where` (`<file>:<line>`) for the caller's own field checks; lazily, so the first bad line, whether not
 * JSON or failing the caller's check, is the one reported. A line that isn't JSON throws `<where>: not JSON`, followed
 * by the parser's reason in parentheses when `reason` is set.
 */
export function* strictJsonLines(
	text: string,
	file: string,
	options: { reason?: boolean } = {},
): Generator<{ row: Record<string, unknown>; where: string }> {
	for (const [i, line] of text.split(/\r?\n/).entries()) {
		if (line.trim() === "") continue;
		const where = `${file}:${i + 1}`;
		let row: Record<string, unknown>;
		try {
			row = JSON.parse(line) as Record<string, unknown>;
		} catch (error) {
			throw new BenchUsageError(`${where}: not JSON${options.reason ? ` (${(error as Error).message})` : ""}`);
		}
		yield { row, where };
	}
}
