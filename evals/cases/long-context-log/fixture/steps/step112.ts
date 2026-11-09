import { step111 } from "./step111.ts";

/** Import step 112: moves each row's sequence number by 2. */
export function step112(rows: readonly number[]): number[] {
	const input = step111(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
