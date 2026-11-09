// A stand-in for mise in `mise.test.ts`: answers `settings get paranoid`, `trust --show`, `env --json` and
// `tasks ls --json` from FAKE_MISE_* variables, and logs each call with its MISE_ENV to FAKE_MISE_LOG.
import { appendFileSync } from "node:fs";

const args = process.argv.slice(2).join(" ");
if (process.env.FAKE_MISE_LOG) {
	appendFileSync(
		process.env.FAKE_MISE_LOG,
		`${JSON.stringify({ args, cwd: process.cwd(), miseEnv: process.env.MISE_ENV })}\n`,
	);
}
const answers: Record<string, string | undefined> = {
	"settings get paranoid": process.env.FAKE_MISE_PARANOID,
	"trust --show": process.env.FAKE_MISE_TRUST,
	"env --json": process.env.FAKE_MISE_ENV_JSON,
	"tasks ls --json": process.env.FAKE_MISE_TASKS,
};
const answer = answers[args];
if (answer === undefined) {
	console.error(`fake mise: no answer for ${args}`);
	process.exit(1);
}
process.stdout.write(answer);
