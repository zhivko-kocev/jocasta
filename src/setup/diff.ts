/**
 * A line diff of two small texts (setup's config files): `  ` kept, `- ` removed, `+ ` added, in order. Plain LCS;
 * the files are a few hundred lines at most.
 */
export function lineDiff(before: string, after: string): string[] {
	const a = before.replace(/\n$/, "").split(/\r?\n/);
	const b = after.replace(/\n$/, "").split(/\r?\n/);
	// lcs[i][j] = longest common subsequence of a[i..] and b[j..].
	const lcs = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
	for (let i = a.length - 1; i >= 0; i--) {
		for (let j = b.length - 1; j >= 0; j--) {
			const row = lcs[i] as number[];
			const next = lcs[i + 1] as number[];
			row[j] = a[i] === b[j] ? (next[j + 1] as number) + 1 : Math.max(next[j] as number, row[j + 1] as number);
		}
	}
	const out: string[] = [];
	let i = 0;
	let j = 0;
	while (i < a.length || j < b.length) {
		if (i < a.length && j < b.length && a[i] === b[j]) {
			out.push(`  ${a[i++]}`);
			j++;
		} else if (j < b.length && (i === a.length || (lcs[i]?.[j + 1] ?? 0) >= (lcs[i + 1]?.[j] ?? 0))) {
			out.push(`+ ${b[j++]}`);
		} else {
			out.push(`- ${a[i++]}`);
		}
	}
	return out;
}
