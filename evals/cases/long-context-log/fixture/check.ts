import { step120 } from "./steps/step120.ts";

// The import runs every row through the 120 steps; step n adds n % 5 to each row.
const rows = Array.from({ length: 250 }, (_, i) => i);
let offset = 0;
for (let n = 1; n <= 120; n++) offset += n % 5;
const output = step120(rows);
if (output.length !== rows.length) {
	console.error(`the import returned ${output.length} rows for ${rows.length}`);
	process.exit(1);
}
const wrong = output.findIndex((value, i) => value !== i + offset);
if (wrong !== -1) {
	console.error(`row ${wrong} is ${output[wrong]}, expected ${wrong + offset}`);
	process.exit(1);
}
console.log(`the import passes all ${rows.length} rows`);
