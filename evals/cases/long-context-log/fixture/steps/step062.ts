import { step061 } from "./step061.ts";

/** Import step 62: moves each row's sequence number by 2. */
export function step062(rows: readonly number[]): number[] {
	const input = step061(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
