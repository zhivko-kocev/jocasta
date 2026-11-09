// H4 (0482): memory isolation with canary facts, in a sandbox Jocasta with two projects A and B.
//
//   bun bench/h4-isolation.ts --out <dir> --project-a <p-…> --pane-b <pane id> [--pane-a <pane id>]
//                             [--canaries 50] [--runs 100] [--skip-plant] [--dry-run]
//
// Plants `--canaries` unique facts in project A's graph as Graphiti episodes (`addEpisodes`, group_id = A's id;
// Graphiti's extraction calls the model through the companion's endpoint, so this step runs in the VM), waits, then
// issues `--runs` queries through the service's MCP endpoint (`graph_search`, as `jocasta mcp` calls it) from B's
// pane, which searches only its own project (0484), and checks every reply for every canary: canaries × queries checks.
// With `--pane-a`, the same queries from A's pane are the positive control (the method can see a canary at all).
// Mnemopi: every other project's `<data>/projects/<id>/mnemopi.db` is scanned for the canary strings (one bank per
// project, `service.ts` `overlayText`); planting into A's bank happens by telling A's pane to remember the canary file.
// Writes h4-isolation.json: leaks, checks and the rule-of-three 95% upper bound when there are none; the canaries
// themselves in h4-canaries.json.
import { randomBytes } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { addEpisodes, type Episode } from "../src/shared/memory/graph/index.ts";
import { dataDir } from "../src/shared/paths.ts";
import { BenchUsageError, flag, parseBenchArgs, runScript } from "./lib/args.ts";
import { conditions, printPlan, writeJson } from "./lib/output.ts";
import { callTool, requireSandbox } from "./lib/sandbox.ts";
import { rate, ruleOfThree } from "./lib/stats.ts";

export interface Canary {
	token: string;
	topic: string;
	fact: string;
}

const TOPICS = ["build server", "release train", "staging database", "deploy key", "feature flag", "test fixture"];

/** `count` canaries: a random token each, inside a plain sentence on one of a few topics. */
export function makeCanaries(
	count: number,
	random: (bytes: number) => string = (n) => randomBytes(n).toString("hex"),
): Canary[] {
	return Array.from({ length: count }, (_, i) => {
		const token = `CANARY-${random(6).toUpperCase()}`;
		const topic = TOPICS[i % TOPICS.length] ?? "build server";
		return { token, topic, fact: `In the acme project the ${topic} codename is ${token}.` };
	});
}

/** The queries, cycling: each topic's codename question, then each canary token asked for directly. */
export function makeQueries(canaries: readonly Canary[], count: number): string[] {
	const pool = [
		...TOPICS.map((topic) => `What is the ${topic} codename?`),
		...canaries.map((canary) => `${canary.token}`),
		...canaries.map((canary) => `codename of the ${canary.topic} in acme`),
	];
	return Array.from({ length: count }, (_, i) => pool[i % pool.length] ?? "codename");
}

/** Which canaries a reply carries. */
export function leaked(reply: string, canaries: readonly Canary[]): string[] {
	return canaries.filter((canary) => reply.includes(canary.token)).map((canary) => canary.token);
}

if (import.meta.main) {
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 100,
			valued: ["--project-a", "--pane-a", "--pane-b", "--canaries", "--wait"],
			booleans: ["--skip-plant"],
		});
		const count = Number(flag(args, "canaries", "50"));
		if (!Number.isInteger(count) || count < 1) throw new BenchUsageError("--canaries needs a whole number");
		const waitMinutes = Number(flag(args, "wait", "10"));
		const projectA = flag(args, "project-a");
		const paneA = flag(args, "pane-a");
		const paneB = flag(args, "pane-b");
		const skipPlant = args.flags.get("skip-plant") === true;
		if (args.dryRun) {
			printPlan(
				"H4 memory isolation",
				[
					skipPlant
						? `reuse the canaries in <out>/h4-canaries.json`
						: `plant ${count} canary facts in project ${projectA ?? "<--project-a>"}'s graph (Graphiti extraction calls the model: VM only), wait ${waitMinutes} min`,
					`${args.runs} graph_search queries from B's pane ${paneB ?? "<--pane-b>"} through POST /api/mcp/graph_search`,
					`check every reply for every canary: ${count * args.runs} checks; 0 leaks → 95% upper bound 3/${count * args.runs}`,
					paneA ? `positive control: the same queries from A's pane ${paneA}` : "no positive control (add --pane-a)",
					"scan every other project's mnemopi.db for the canary strings",
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		requireSandbox();
		if (!projectA || !paneB) throw new BenchUsageError("--project-a and --pane-b are required");
		const started = conditions(["omp"]);
		const canaryFile = join(out, "h4-canaries.json");
		let canaries: Canary[];
		if (skipPlant) {
			if (!existsSync(canaryFile)) throw new BenchUsageError(`--skip-plant needs ${canaryFile} from an earlier run`);
			canaries = JSON.parse(readFileSync(canaryFile, "utf8")).canaries as Canary[];
		} else {
			canaries = makeCanaries(count);
			writeJson(out, "h4-canaries", { projectA, canaries });
			const now = new Date().toISOString();
			const episodes: Episode[] = canaries.map((canary, i) => ({
				name: `h4 canary ${i + 1}`,
				body: canary.fact,
				source: "h4 benchmark",
				reference_time: now,
			}));
			await addEpisodes(projectA, episodes);
			console.log(`planted ${canaries.length} canaries in ${projectA}; waiting ${waitMinutes} min`);
			await Bun.sleep(waitMinutes * 60_000);
		}
		const queries = makeQueries(canaries, args.runs);
		const ask = async (paneId: string) => {
			const results: { query: string; leaked: string[]; isError: boolean }[] = [];
			for (const query of queries) {
				const reply = await callTool("graph_search", { query, limit: 20 }, { paneId });
				results.push({ query, leaked: leaked(reply.text, canaries), isError: reply.isError === true });
			}
			return results;
		};
		const fromB = await ask(paneB);
		const fromA = paneA ? await ask(paneA) : null;
		const checks = canaries.length * queries.length;
		const leaks = fromB.reduce((sum, result) => sum + result.leaked.length, 0);
		const projectsDir = join(dataDir(), "projects");
		const bankLeaks = existsSync(projectsDir)
			? readdirSync(projectsDir)
					.filter((id) => id !== projectA && existsSync(join(projectsDir, id, "mnemopi.db")))
					.map((id) => {
						const bytes = readFileSync(join(projectsDir, id, "mnemopi.db")).toString("latin1");
						return {
							project: id,
							leaked: canaries.filter((canary) => bytes.includes(canary.token)).map((c) => c.token),
						};
					})
			: [];
		const file = writeJson(out, "h4-isolation", {
			benchmark: "H4",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			canaries: canaries.length,
			queries: queries.length,
			checks,
			graphLeaks: leaks,
			graphLeakRate: leaks / checks,
			upperBound95: leaks === 0 ? ruleOfThree(checks) : null,
			erroredQueries: fromB.filter((result) => result.isError).length,
			positiveControl: fromA ? rate(fromA.map((result) => result.leaked.length > 0)) : null,
			mnemopiBanks: bankLeaks,
			leakedQueries: fromB.filter((result) => result.leaked.length > 0),
		});
		console.log(`${leaks} leaks in ${checks} checks; wrote ${file}`);
	});
}
