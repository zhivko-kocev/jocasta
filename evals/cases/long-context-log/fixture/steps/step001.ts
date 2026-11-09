/** Import step 1: offsets each row's sequence number by 1. */
export function step001(rows: readonly number[]): number[] {
	const input = rows;
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
