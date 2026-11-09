import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

// Built from parts, so a rename across the repo leaves this check intact.
const oldName = ["fetch", "Usr"].join("");
const root = import.meta.dir;
const found: string[] = [];
for (const entry of readdirSync(root, { recursive: true, withFileTypes: true })) {
	const path = join(entry.parentPath, entry.name);
	const rel = relative(root, path);
	if (
		!entry.isFile() ||
		rel === "check-names.ts" ||
		rel.split(/[\\/]/).some((part) => part === ".git" || part === "node_modules")
	)
		continue;
	if (readFileSync(path, "utf8").includes(oldName)) found.push(rel);
}
if (found.length > 0) {
	console.error(`${oldName} is still used in: ${found.join(", ")}`);
	process.exit(1);
}
console.log("no old names left");
