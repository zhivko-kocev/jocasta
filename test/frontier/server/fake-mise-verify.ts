// A stand-in for mise in `pane-check.test.ts`: a trusted project whose `verify` task is the check. `run -c verify`
// counts itself in `$FAKE_VERIFY_DIR/runs`, waits `delay` milliseconds, then exits with `exit` (both files in that
// folder, read when it starts; 0 when absent). A run that starts while another is under way adds a line to `overlaps`.
import { appendFileSync, existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const args = process.argv.slice(2).join(" ");
const dir = process.env.FAKE_VERIFY_DIR ?? "";
const number = (name: string): number =>
	existsSync(join(dir, name)) ? Number(readFileSync(join(dir, name), "utf8").trim()) : 0;

if (args === "run -c verify") {
	const [delay, exit] = [number("delay"), number("exit")];
	const active = join(dir, "active");
	if (existsSync(active)) appendFileSync(join(dir, "overlaps"), "overlap\n");
	writeFileSync(active, "");
	appendFileSync(join(dir, "runs"), "run\n");
	await Bun.sleep(delay);
	rmSync(active, { force: true });
	console.error("verify: boom");
	process.exit(exit);
}
const answers: Record<string, string> = {
	"settings get paranoid": "true\n",
	"trust --show": "",
	"tasks ls --json": '[{"name":"verify"}]',
	"env --json": "{}",
};
const answer = /^trust (?!--)/.test(args) ? "" : answers[args];
if (answer === undefined) {
	console.error(`fake mise: no answer for ${args}`);
	process.exit(1);
}
process.stdout.write(answer);
