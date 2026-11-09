import type { NewEvent } from "../events/queue.ts";
import { webLink } from "../link.ts";
import { type RunCli, runJson } from "./cli.ts";

interface Notification {
	id: string;
	reason?: string;
	subject?: { title?: string; url?: string | null; type?: string };
	repository?: { full_name?: string };
}

interface SearchPr {
	number: number;
	title: string;
	url: string;
	repository: { nameWithOwner: string };
}

interface Check {
	name?: string;
	bucket?: string;
	link?: string;
}

/** The PRs whose checks are read each round, his most recently updated first. */
const MAX_PRS = 10;

/** `https://api.github.com/repos/o/r/pulls/42` → `https://github.com/o/r/pull/42`. */
function webUrl(api: string | null | undefined): string | undefined {
	const match = api ? /^https:\/\/api\.github\.com\/repos\/([^/]+\/[^/]+)\/pulls\/(\d+)$/.exec(api) : null;
	return match ? `https://github.com/${match[1]}/pull/${match[2]}` : undefined;
}

/** The kinds the GitHub poller owns; open events of these kinds it no longer sees are resolved. */
export const GITHUB_KINDS = ["review", "pipeline"] as const;

/**
 * One GitHub round (0301): reviews requested from him (`gh api notifications`, normal, 0298) and failing checks on his
 * open PRs (`gh search prs`, then `gh pr checks`; urgent, 0303: a red pipeline on his branch).
 */
export async function pollGitHub(run: RunCli): Promise<NewEvent[]> {
	const events: NewEvent[] = [];
	const notifications = await runJson<Notification[]>(run, "gh", ["api", "notifications"], []);
	for (const n of notifications) {
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
	const prs = await runJson<SearchPr[]>(
		run,
		"gh",
		[
			"search",
			"prs",
			"--author",
			"@me",
			"--state",
			"open",
			"--sort",
			"updated",
			"--limit",
			String(MAX_PRS),
			"--json",
			"number,title,url,repository",
		],
		[],
	);
	for (const pr of prs) {
		const repo = pr.repository.nameWithOwner;
		// `gh pr checks` exits 1 when some failed and 8 while some are pending; its JSON is on stdout either way.
		const checks = await runJson<Check[]>(
			run,
			"gh",
			["pr", "checks", String(pr.number), "-R", repo, "--json", "name,bucket,link"],
			[],
			[1, 8],
		);
		const failed = checks.filter((c) => c.bucket === "fail");
		if (failed.length === 0) continue;
		// A check's link comes from the CI service: kept only as a well-formed http(s) link (SE-6).
		const url = webLink(failed[0]?.link) ?? webLink(pr.url);
		events.push({
			sourceId: `github:pipeline:${repo}#${pr.number}`,
			source: "github",
			kind: "pipeline",
			priority: "urgent",
			text: `Checks failing — ${repo} #${pr.number} ${pr.title}: ${failed.map((c) => c.name ?? "?").join(", ")}`,
			...(url ? { url } : {}),
		});
	}
	return events;
}
