import { sum } from "./sum.ts";

if (sum(2, 3) !== 5) {
	console.error(`sum(2, 3) is ${sum(2, 3)}, expected 5`);
	process.exit(1);
}
