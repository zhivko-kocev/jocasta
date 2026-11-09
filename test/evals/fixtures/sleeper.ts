// An omp that never finishes: `--version` answers, as the runner asks once per run; otherwise it starts a sleeping
// child (so the trial is a process tree), records the child's pid in `JOCASTA_TEST_SLEEPER_PID_FILE`, prints one JSON
// line and sleeps.
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

if (process.argv.includes("--version")) {
	console.log("omp/0.0.0-sleeper");
	process.exit(0);
}
const child = spawn(process.execPath, ["-e", "setTimeout(() => {}, 120_000)"], { stdio: "ignore" });
writeFileSync(process.env.JOCASTA_TEST_SLEEPER_PID_FILE ?? "", String(child.pid));
console.log(JSON.stringify({ type: "agent_start" }));
setTimeout(() => {}, 120_000);
