import { spawnSync } from "node:child_process";
import { join } from "node:path";

const cli = join(import.meta.dir, "cli.ts");
const run = (args: string[]) => spawnSync(process.execPath, [cli, ...args], { encoding: "utf8" }).stdout.trim();

let failed = 0;
const plain = run(["acme"]);
if (plain !== "Hello, acme!") {
	console.error(`plain: expected "Hello, acme!", got ${JSON.stringify(plain)}`);
	failed++;
}
for (const args of [
	["--json", "client-a"],
	["client-a", "--json"],
]) {
	const out = run(args);
	let parsed: unknown;
	try {
		parsed = JSON.parse(out);
	} catch {
		parsed = undefined;
	}
	if (JSON.stringify(parsed) !== JSON.stringify({ greeting: "Hello, client-a!" })) {
		console.error(`${args.join(" ")}: expected {"greeting":"Hello, client-a!"}, got ${JSON.stringify(out)}`);
		failed++;
	}
}
if (failed > 0) process.exit(1);
console.log("cli: all forms pass");
