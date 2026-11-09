import { step056 } from "./step056.ts";

/** Import step 57: moves each row's sequence number by 2. */
export function step057(rows: readonly number[]): number[] {
	const input = step056(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
