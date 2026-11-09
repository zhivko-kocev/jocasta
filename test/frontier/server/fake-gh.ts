// A stand-in for GitHub CLI in the service's tests, run as `bun fake-gh.ts <state.json> <gh args…>`. The state file's
// `{auth?: "fail", head?: string, hang?: true}`: `auth status` passes unless `auth` is "fail"; `pr view <branch> --json …`
// prints an open PR whose head is `head` (exit 1 when unset, as gh does with no PR); `hang` never answers (CO-8).
import { readFileSync } from "node:fs";

const [stateFile = "", ...args] = process.argv.slice(2);
let state: { auth?: string; head?: string; hang?: boolean } = {};
try {
	state = JSON.parse(readFileSync(stateFile, "utf8")) as typeof state;
} catch {
	// No state yet: logged in, no PR.
}
const command = args.slice(0, 2).join(" ");
if (state.hang) {
	// A gh waiting on something that never comes: alive until it is killed.
	setInterval(() => {}, 60_000);
} else if (command === "auth status") {
	if (state.auth === "fail") {
		process.stderr.write("You are not logged into any GitHub hosts. To log in, run: gh auth login\n");
		process.exit(1);
	}
	process.stdout.write("github.com\n  Logged in to github.com account test\n");
} else if (command === "pr view") {
	if (!state.head) {
		process.stderr.write(`no pull requests found for branch "${args[2]}"\n`);
		process.exit(1);
	}
	process.stdout.write(
		`${JSON.stringify({ url: "https://github.com/test/demo/pull/7", state: "OPEN", headRefOid: state.head })}\n`,
	);
}
