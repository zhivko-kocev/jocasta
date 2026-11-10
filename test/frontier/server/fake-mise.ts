// A stand-in for mise in `mise.test.ts`: answers `settings get paranoid`, `trust --show`, `env --json` and
// `tasks ls --json` from FAKE_MISE_* variables, and logs each call with its MISE_ENV to FAKE_MISE_LOG. `trust --show`
// answers from FAKE_MISE_TRUST_IN (JSON: folder → answer) when it names the folder mise runs in, else FAKE_MISE_TRUST;
// `trust <file>` succeeds.
import { appendFileSync } from "node:fs";
import { pathKey, realPath } from "../../../src/shared/pathkey.ts";

const args = process.argv.slice(2).join(" ");
if (process.env.FAKE_MISE_LOG) {
	appendFileSync(
		process.env.FAKE_MISE_LOG,
		`${JSON.stringify({ args, cwd: process.cwd(), miseEnv: process.env.MISE_ENV })}\n`,
	);
}
const byFolder: Record<string, string> = JSON.parse(process.env.FAKE_MISE_TRUST_IN ?? "{}");
const here = Object.entries(byFolder).find(([dir]) => pathKey(realPath(dir)) === pathKey(realPath(process.cwd())));
const answers: Record<string, string | undefined> = {
	"settings get paranoid": process.env.FAKE_MISE_PARANOID,
	"trust --show": here ? here[1] : process.env.FAKE_MISE_TRUST,
	"env --json": process.env.FAKE_MISE_ENV_JSON,
	"tasks ls --json": process.env.FAKE_MISE_TASKS,
};
const answer = /^trust (?!--)/.test(args) ? "" : answers[args];
if (answer === undefined) {
	console.error(`fake mise: no answer for ${args}`);
	process.exit(1);
}
process.stdout.write(answer);
