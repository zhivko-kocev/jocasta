// H4 (0482, 0502, 0576): memory isolation with canary facts, in a sandbox Jocasta on his PC with two projects A and B.
//
//   bun bench/h4-isolation.ts --out <dir> --project-a <p-…> --pane-a <pane id> --pane-b <pane id>
//                             [--canaries 50] [--runs 100] [--wait 10] [--plant-timeout 30] [--skip-plant] [--dry-run]
//
// Plants `--canaries` unique facts in project A's graph through the service (0576): H4 starts a thread `h4 plant` in A
// as `jocasta thread new` does, whose brief tells it to call `report_result` once with exactly the canaries as
// decisions; the service posts them as one episode to A's graph with its own Graphiti token and model endpoint
// (0539). The thread opens in plan mode, so he approves its plan. Once `jocasta thread list` shows its report, H4 waits
// `--wait` minutes for Graphiti's extraction, then issues `--runs` queries through the service's MCP endpoint
// (`graph_search`, as `jocasta mcp` calls it with the pane's key from `<data>/panes.json`) from B's pane, which
// searches only its own project (0484), and checks every answered reply for every canary: canaries × answered queries
// checks. The same queries from A's pane are the positive control: a bound is given only when it found canaries in
// at least one reply, else the run is `inconclusive`.
// Mnemopi: every other project's `<data>/projects/<id>/mnemopi.db` and its `-wal` are scanned for the canary strings
// (one bank per project, `service.ts` `overlayText`).
// Writes h4-isolation.json: leaks, answered checks, the positive control and the rule-of-three 95% upper bound when
// there are none; the canaries themselves in h4-canaries.json.
import { randomBytes } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { LaunchedPanes } from "../src/frontier/server/boss.ts";
import { BenchUsageError, flag, parseBenchArgs, runScript } from "./lib/args.ts";
import { conditions, printPlan, writeJson } from "./lib/output.ts";
import { callTool, requireSandbox } from "./lib/sandbox.ts";
import { rate, ruleOfThree } from "./lib/stats.ts";
import { listThreads, startThread } from "./lib/threads.ts";
import { waitFor } from "./lib/timing.ts";

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

/** The title of the thread that plants the canaries. */
export const PLANT_TITLE = "h4 plant";

/**
 * The `h4 plant` thread's brief: one `report_result` call whose decisions are exactly the canaries, which the service
 * posts to A's graph as `- <topic> codename: <token>. Why: H4 canary` lines (0539).
 */
export function plantBrief(canaries: readonly Canary[]): string {
	const decisions = canaries.map((canary) => ({
		question: `${canary.topic} codename`,
		choice: canary.token,
		why: "H4 canary",
	}));
	return [
		"Benchmark H4: change no file and run no command. Call the report_result tool exactly once, with outcome done,",
		'summary "H4 canaries", no files, no tests, no questions, no follow-ups, and exactly these decisions, unchanged:',
		JSON.stringify(decisions),
		"Then stop.",
	].join("\n");
}

export interface QueryResult {
	query: string;
	leaked: string[];
	isError: boolean;
}

export interface H4Score {
	answeredQueries: number;
	erroredQueries: number;
	checks: number;
	graphLeaks: number;
	graphLeakRate: number | null;
	/** Answered queries from A's pane whose reply carried a canary, of A's answered queries. */
	positiveControl: { hits: number; answered: number };
	/** No canary was findable from A's pane, or nothing was answered: no bound. */
	inconclusive: boolean;
	upperBound95: number | null;
}

/**
 * H4's figures (0576): only answered queries count as checks, and the rule-of-three bound holds only when the positive
 * control found a canary at all and B leaked none.
 */
export function scoreH4(fromB: readonly QueryResult[], fromA: readonly QueryResult[], canaries: number): H4Score {
	const answeredB = fromB.filter((result) => !result.isError);
	const answeredA = fromA.filter((result) => !result.isError);
	const checks = canaries * answeredB.length;
	const leaks = answeredB.reduce((sum, result) => sum + result.leaked.length, 0);
	const hits = answeredA.filter((result) => result.leaked.length > 0).length;
	const inconclusive = hits === 0 || checks === 0;
	return {
		answeredQueries: answeredB.length,
		erroredQueries: fromB.length - answeredB.length,
		checks,
		graphLeaks: leaks,
		graphLeakRate: checks === 0 ? null : leaks / checks,
		positiveControl: { hits, answered: answeredA.length },
		inconclusive,
		upperBound95: !inconclusive && leaks === 0 ? ruleOfThree(checks) : null,
	};
}

/** The run's line, as H4 prints it. */
export function h4Line(score: H4Score): string {
	const n = (value: number) => value.toLocaleString("en-US");
	const control =
		score.positiveControl.hits === 0
			? "positive control found no canary: inconclusive"
			: `positive control found canaries in ${score.positiveControl.hits}/${score.positiveControl.answered} queries`;
	const bound = score.upperBound95 === null ? "" : `; 95% upper bound 3/${n(score.checks)}`;
	return `${score.graphLeaks} leaks in ${n(score.checks)} answered checks (${score.erroredQueries} queries errored); ${control}${bound}`;
}

/** Canary tokens in a project's mnemopi bank: `mnemopi.db` and its write-ahead log (mnemopi runs WAL). */
export function bankLeaks(projectDir: string, canaries: readonly Canary[]): string[] {
	const text = ["mnemopi.db", "mnemopi.db-wal"]
		.map((name) => join(projectDir, name))
		.filter((path) => existsSync(path))
		.map((path) => readFileSync(path).toString("latin1"))
		.join("\n");
	return canaries.filter((canary) => text.includes(canary.token)).map((canary) => canary.token);
}

if (import.meta.main) {
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 100,
			valued: ["--project-a", "--pane-a", "--pane-b", "--canaries", "--wait", "--plant-timeout"],
			booleans: ["--skip-plant"],
		});
		const count = Number(flag(args, "canaries", "50"));
		if (!Number.isInteger(count) || count < 1) throw new BenchUsageError("--canaries needs a whole number");
		const waitMinutes = Number(flag(args, "wait", "10"));
		const plantMinutes = Number(flag(args, "plant-timeout", "30"));
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
						: `start the thread "${PLANT_TITLE}" in ${projectA ?? "<--project-a>"} (jocasta thread new; he approves its plan) to hand in report_result with ${count} canaries as decisions, which the service posts to A's graph; wait up to ${plantMinutes} min for its report, then ${waitMinutes} min for Graphiti's extraction`,
					`${args.runs} graph_search queries from B's pane ${paneB ?? "<--pane-b>"} through POST /api/mcp/graph_search (the pane's key from <data>/panes.json)`,
					`check every answered reply for every canary: up to ${count * args.runs} checks; 0 leaks → 95% upper bound 3/<answered checks>`,
					`positive control: the same queries from A's pane ${paneA ?? "<--pane-a>"}; no canary found there → inconclusive, no bound`,
					"scan every other project's mnemopi.db and mnemopi.db-wal for the canary strings",
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		const sandbox = requireSandbox();
		if (!projectA || !paneA || !paneB) throw new BenchUsageError("--project-a, --pane-a and --pane-b are required");
		const launched = new LaunchedPanes(join(sandbox.dataDir, "panes.json"));
		const keyOf = (paneId: string) => {
			const key = launched.keyOf(paneId);
			if (!key)
				throw new BenchUsageError(`pane ${paneId} has no key in <data>/panes.json: pass a pane Jocasta started`);
			return key;
		};
		const keyA = keyOf(paneA);
		const keyB = keyOf(paneB);
		const started = conditions(["omp", "jocasta"]);
		const canaryFile = join(out, "h4-canaries.json");
		let canaries: Canary[];
		if (skipPlant) {
			if (!existsSync(canaryFile)) throw new BenchUsageError(`--skip-plant needs ${canaryFile} from an earlier run`);
			canaries = JSON.parse(readFileSync(canaryFile, "utf8")).canaries as Canary[];
		} else {
			if (existsSync(canaryFile)) throw new BenchUsageError(`${canaryFile} exists; results are never overwritten`);
			canaries = makeCanaries(count);
			writeJson(out, "h4-canaries", { projectA, canaries });
			const branch = await startThread(projectA, PLANT_TITLE, plantBrief(canaries));
			console.log(`started "${PLANT_TITLE}" on ${branch}; approve its plan in its pane`);
			const reported = await waitFor(
				async () => (await listThreads(projectA)).some((line) => line.branch === branch && line.reported !== null),
				{ timeoutMs: plantMinutes * 60_000, intervalMs: 10_000 },
			);
			if (reported === null)
				throw new Error(`"${PLANT_TITLE}" handed in no report within ${plantMinutes} min; nothing was planted`);
			console.log(`planted ${canaries.length} canaries in ${projectA}; waiting ${waitMinutes} min`);
			await Bun.sleep(waitMinutes * 60_000);
		}
		const queries = makeQueries(canaries, args.runs);
		const ask = async (paneKey: string) => {
			const results: QueryResult[] = [];
			for (const query of queries) {
				const reply = await callTool("graph_search", { query, limit: 20 }, { paneKey });
				results.push({ query, leaked: leaked(reply.text, canaries), isError: reply.isError === true });
			}
			return results;
		};
		const fromB = await ask(keyB);
		const fromA = await ask(keyA);
		const score = scoreH4(fromB, fromA, canaries.length);
		const projectsDir = join(sandbox.dataDir, "projects");
		const banks = existsSync(projectsDir)
			? readdirSync(projectsDir)
					.filter((id) => id !== projectA)
					.map((id) => ({ project: id, leaked: bankLeaks(join(projectsDir, id), canaries) }))
			: [];
		const file = writeJson(out, "h4-isolation", {
			benchmark: "H4",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			canaries: canaries.length,
			queries: queries.length,
			...score,
			positiveControlRate: rate(fromA.filter((r) => !r.isError).map((r) => r.leaked.length > 0)),
			mnemopiBanks: banks,
			leakedQueries: fromB.filter((result) => result.leaked.length > 0),
		});
		console.log(`${h4Line(score)}; wrote ${file}`);
	});
}
