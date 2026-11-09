import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Runs slugify.test.ts against each broken copy of slugify.ts in mutants/; passes only when the tests catch them all.
const tests = join(import.meta.dir, "slugify.test.ts");
if (!existsSync(tests)) {
	console.error("slugify.test.ts is missing");
	process.exit(1);
}
const survivors: string[] = [];
for (const mutant of ["a", "b", "c"]) {
	const dir = mkdtempSync(join(tmpdir(), `slugify-mutant-${mutant}-`));
	try {
		copyFileSync(tests, join(dir, "slugify.test.ts"));
		copyFileSync(join(import.meta.dir, "mutants", `${mutant}.ts`), join(dir, "slugify.ts"));
		const run = spawnSync(process.execPath, ["test", "./slugify.test.ts"], { cwd: dir, stdio: "ignore" });
		if (run.status === 0) survivors.push(mutant);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
}
if (survivors.length > 0) {
	console.error(`the tests pass against mutant ${survivors.join(", ")}`);
	process.exit(1);
}
console.log("the tests catch every mutant");
