import type { NewEvent, PollRound } from "../events/queue.ts";
import { webLink } from "../link.ts";
import { type RunCli, runJson } from "./cli.ts";

interface Notification {
	id: string;
	reason?: string;
	subject?: { title?: string; url?: string | null; type?: string };
	repository?: { full_name?: string };
}

/** A status rollup context: a check run, or a commit status (the other fields absent). */
interface CheckContext {
	name?: string;
	conclusion?: string | null;
	status?: string;
	detailsUrl?: string | null;
	startedAt?: string | null;
	checkSuite?: { workflowRun?: { event?: string; workflow?: { name?: string } } | null } | null;
	context?: string;
	state?: string;
	targetUrl?: string | null;
}

interface OpenPr {
	url?: string;
}

interface RecentPr {
	number?: number;
	title?: string;
	url?: string;
	repository?: { nameWithOwner?: string };
	commits?: {
		nodes?: ({ commit?: { statusCheckRollup?: { contexts?: { nodes?: (CheckContext | null)[] } } | null } } | null)[];
	};
}

interface Search<T> {
	nodes?: (T | null)[];
}

/** His open PRs read each round to tell which are still open; resolving a pipeline event needs its PR gone (SHR-8). */
const MAX_OPEN = 100;
/** The PRs whose checks are read each round, his most recently updated first. */
const MAX_PRS = 10;

/**
 * One GraphQL request (PERF-8) for his open PRs, most recently updated first: the first 100 for the open set, and the
 * newest 10 with their last commit's checks (as `gh pr checks` reads them). No `"` or `%`: runCli refuses those.
 */
const PRS_QUERY = [
	"query($q: String!) {",
	`open: search(type: ISSUE, query: $q, first: ${MAX_OPEN}) { nodes { ... on PullRequest { url } } }`,
	`recent: search(type: ISSUE, query: $q, first: ${MAX_PRS}) { nodes { ... on PullRequest {`,
	"number title url repository { nameWithOwner }",
	"commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes {",
	"... on CheckRun { name conclusion status detailsUrl startedAt checkSuite { workflowRun { event workflow { name } } } }",
	"... on StatusContext { context state targetUrl }",
	"} } } } } }",
	"} } }",
	"}",
].join(" ");
/** `gh search prs --author @me --state open --sort updated` as a search query. */
const PRS_SEARCH = "is:pr is:open author:@me sort:updated-desc";

/** `https://api.github.com/repos/o/r/pulls/42` → `https://github.com/o/r/pull/42`. */
function webUrl(api: string | null | undefined): string | undefined {
	const match = api ? /^https:\/\/api\.github\.com\/repos\/([^/]+\/[^/]+)\/pulls\/(\d+)$/.exec(api) : null;
	return match ? `https://github.com/${match[1]}/pull/${match[2]}` : undefined;
}

/** `https://github.com/o/r/pull/42` → `o/r#42`, the key of a pipeline event's source id. */
function prKey(url: string | undefined): string | undefined {
	const match = url ? /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)$/.exec(url) : null;
	return match ? `${match[1]}#${match[2]}` : undefined;
}

/**
 * The checks `gh pr checks` puts in its `fail` bucket (cli/cli pkg/cmd/pr/checks/aggregate.go): only the newest run of
 * each check name, workflow and event (a re-run replaces a failed run), and of each status context; failing when its
 * state (a status's state, a completed run's conclusion) is ERROR, FAILURE, TIMED_OUT or ACTION_REQUIRED.
 */
function failingChecks(contexts: readonly CheckContext[]): { name: string; link?: string }[] {
	const newestFirst = [...contexts].sort(
		(a, b) => (Date.parse(b.startedAt ?? "") || 0) - (Date.parse(a.startedAt ?? "") || 0),
	);
	const keys = new Set<string>();
	const failed: { name: string; link?: string }[] = [];
	for (const c of newestFirst) {
		const run = c.checkSuite?.workflowRun;
		const key = c.context
			? `status:${c.context}`
			: `run:${c.name ?? ""}/${run?.workflow?.name ?? ""}/${run?.event ?? ""}`;
		if (keys.has(key)) continue;
		keys.add(key);
		const state = c.state || (c.status === "COMPLETED" ? (c.conclusion ?? "") : (c.status ?? ""));
		if (!["ERROR", "FAILURE", "TIMED_OUT", "ACTION_REQUIRED"].includes(state)) continue;
		const link = c.detailsUrl || c.targetUrl || undefined;
		failed.push({ name: c.name || c.context || "?", ...(link ? { link } : {}) });
	}
	return failed;
}

/** The kinds the GitHub poller owns; open events of these kinds it saw go away are resolved. */
export const GITHUB_KINDS = ["review", "pipeline"] as const;

/**
 * One GitHub round (0301): reviews requested from him (every page of `gh api notifications`, normal, 0298) and failing
 * checks on his open PRs (one `gh api graphql` search, PERF-8; urgent, 0303: a red pipeline on his branch). A pipeline
 * event is observed (SHR-8) only when its PR's checks were read, or the PR is no longer among his open ones.
 */
export async function pollGitHub(run: RunCli): Promise<PollRound> {
	const events: NewEvent[] = [];
	const pages = await runJson<Notification[][]>(run, "gh", ["api", "notifications", "--paginate", "--slurp"], []);
	for (const n of pages.flat()) {
		if (n.reason !== "review_requested" || n.subject?.type !== "PullRequest") continue;
		const url = webUrl(n.subject.url);
		const repo = n.repository?.full_name ?? "GitHub";
		const number = url?.split("/").at(-1);
		events.push({
			sourceId: `github:review:${repo}#${number ?? n.id}`,
			source: "github",
			kind: "review",
			priority: "normal",
			text: `Review requested — ${n.subject.title ?? "a pull request"} (${repo}${number ? ` #${number}` : ""})`,
			...(url ? { url } : {}),
		});
	}
	const answer = await runJson<{ data?: { open?: Search<OpenPr>; recent?: Search<RecentPr> } }>(
		run,
		"gh",
		["api", "graphql", "-f", `query=${PRS_QUERY}`, "-f", `q=${PRS_SEARCH}`],
		{},
	);
	const open = new Set<string>();
	for (const pr of answer.data?.open?.nodes ?? []) {
		const key = prKey(pr?.url);
		if (key) open.add(key);
	}
	const checked = new Set<string>();
	for (const pr of answer.data?.recent?.nodes ?? []) {
		const repo = pr?.repository?.nameWithOwner;
		if (!pr || !repo || pr.number === undefined) continue;
		checked.add(`${repo}#${pr.number}`);
		const contexts = pr.commits?.nodes?.[0]?.commit?.statusCheckRollup?.contexts?.nodes ?? [];
		const failed = failingChecks(contexts.filter((c): c is CheckContext => c !== null));
		if (failed.length === 0) continue;
		// A check's link comes from the CI service: kept only as a well-formed http(s) link (SE-6).
		const url = webLink(failed[0]?.link) ?? webLink(pr.url);
		events.push({
			sourceId: `github:pipeline:${repo}#${pr.number}`,
			source: "github",
			kind: "pipeline",
			priority: "urgent",
			text: `Checks failing — ${repo} #${pr.number} ${pr.title ?? ""}: ${failed.map((c) => c.name).join(", ")}`,
			...(url ? { url } : {}),
		});
	}
	const observed = (sourceId: string): boolean => {
		const pipeline = /^github:pipeline:(.+)$/.exec(sourceId)?.[1];
		return pipeline === undefined || checked.has(pipeline) || !open.has(pipeline);
	};
	return { events, observed, failures: [] };
}
