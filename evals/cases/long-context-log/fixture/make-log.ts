import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// Writes logs/import.log: the nightly import's ~2 MB log for acme, with one ERROR line near the middle.
const LINES = 24_000;
let seed = 42;
const next = (max: number) => {
	seed = (seed * 1_103_515_245 + 12_345) % 2_147_483_648;
	return seed % max;
};
const start = Date.UTC(2026, 9, 11, 1, 0, 0);
const lines: string[] = [];
for (let i = 0; i < LINES; i++) {
	const at = new Date(start + i * 97).toISOString();
	const batch = 1000 + Math.floor(i / 120);
	const step = `step${String((i % 120) + 1).padStart(3, "0")}`;
	// step087's line in the batch nearest the middle.
	if (i === 12_086) {
		lines.push(
			`${at} ERROR import batch=${batch} step=step087 rows_in=250 rows_out=249: row count changed; import aborted`,
		);
	} else if (next(50) === 0) {
		lines.push(`${at} WARN  import batch=${batch} step=${step} slow: ${200 + next(800)} ms`);
	} else {
		lines.push(`${at} INFO  import batch=${batch} step=${step} rows=250 ok in ${1 + next(40)} ms`);
	}
}
mkdirSync(join(import.meta.dir, "logs"), { recursive: true });
writeFileSync(join(import.meta.dir, "logs", "import.log"), `${lines.join("\n")}\n`);
