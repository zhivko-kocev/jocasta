import type { NewEvent, PollRound } from "../events/queue.ts";
import { type RunCli, runJson } from "./cli.ts";

interface AzPr {
	pullRequestId: number;
	title: string;
	repository?: { name?: string; project?: { name?: string } };
	reviewers?: { uniqueName?: string; vote?: number }[];
}

interface AzRun {
	id: number;
	result?: string | null;
	status?: string;
	sourceBranch?: string;
	finishTime?: string | null;
	definition?: { id?: number; name?: string };
	requestedFor?: { uniqueName?: string };
	project?: { name?: string };
}

/** The kinds the Azure DevOps poller owns. */
export const AZURE_KINDS = ["review", "pipeline"] as const;

/** Runs read per project each round, newest finished first. */
const RUNS_PER_PROJECT = "30";
const TRUNKS = ["refs/heads/main", "refs/heads/master"];

/** His sign-in name, the reviewer and requester `az` reports. */
async function me(run: RunCli): Promise<string> {
	return (await run("az", ["account", "show", "--query", "user.name", "--output", "tsv"])).trim().toLowerCase();
}

/**
 * One Azure DevOps round for each org in config (0301): active PRs where he is a reviewer and hasn't voted (normal,
 * 0298), and the latest run of each pipeline and branch that failed on main or master, or on a branch he pushed
 * (urgent, 0303). A listing that fails (an org, or one project the account can't read) leaves that org's or project's
 * events out of the observed scope, so they stay as they were, and is returned as a failure; the rest of the round
 * goes on (SHR-8).
 */
export async function pollAzure(run: RunCli, orgs: readonly string[]): Promise<PollRound> {
	const events: NewEvent[] = [];
	/** Source id prefixes whose listing failed this round. */
	const unseen: string[] = [];
	const failures: string[] = [];
	const failed = (prefix: string, error: unknown): void => {
		unseen.push(prefix);
		const message = (error as Error).message;
		if (!failures.includes(message)) failures.push(message);
	};
	const observed = (sourceId: string): boolean => !unseen.some((prefix) => sourceId.startsWith(prefix));
	if (orgs.length === 0) return { events, observed, failures };
	const user = await me(run);
	for (const org of orgs) {
		const base = org.replace(/\/+$/, "");
		try {
			const prs = await runJson<AzPr[]>(
				run,
				"az",
				["repos", "pr", "list", "--organization", base, "--reviewer", user, "--status", "active", "--output", "json"],
				[],
			);
			for (const pr of prs) {
				const mine = pr.reviewers?.find((r) => r.uniqueName?.toLowerCase() === user);
				if (mine && (mine.vote ?? 0) !== 0) continue;
				const project = pr.repository?.project?.name ?? "";
				const repo = pr.repository?.name ?? "";
				events.push({
					sourceId: `azure:review:${base}/${project}/${repo}#${pr.pullRequestId}`,
					source: "azure",
					kind: "review",
					priority: "normal",
					text: `Review requested — ${pr.title} (${project}/${repo} !${pr.pullRequestId})`,
					url: `${base}/${encodeURIComponent(project)}/_git/${encodeURIComponent(repo)}/pullrequest/${pr.pullRequestId}`,
				});
			}
		} catch (error) {
			failed(`azure:review:${base}/`, error);
		}
		let projects: { value?: { name: string }[] };
		try {
			projects = await runJson<{ value?: { name: string }[] }>(
				run,
				"az",
				["devops", "project", "list", "--organization", base, "--output", "json"],
				{},
			);
		} catch (error) {
			failed(`azure:pipeline:${base}/`, error);
			continue;
		}
		for (const { name } of projects.value ?? []) {
			if (/["%]/.test(name)) continue;
			let runs: AzRun[];
			try {
				runs = await runJson<AzRun[]>(
					run,
					"az",
					[
						"pipelines",
						"runs",
						"list",
						"--organization",
						base,
						"--project",
						name,
						"--top",
						RUNS_PER_PROJECT,
						"--query-order",
						"FinishTimeDesc",
						"--output",
						"json",
					],
					[],
				);
			} catch (error) {
				failed(`azure:pipeline:${base}/${name}/`, error);
				continue;
			}
			// The newest finished run of each pipeline and branch decides whether it is red.
			const latest = new Map<string, AzRun>();
			for (const r of runs) {
				if (r.status !== "completed" || !r.sourceBranch) continue;
				const key = `${r.definition?.id ?? "?"}:${r.sourceBranch}`;
				if (!latest.has(key)) latest.set(key, r);
			}
			for (const [key, r] of latest) {
				const branch = r.sourceBranch as string;
				const his = r.requestedFor?.uniqueName?.toLowerCase() === user;
				if (r.result !== "failed" || !(TRUNKS.includes(branch) || his)) continue;
				const short = branch.replace(/^refs\/heads\//, "");
				events.push({
					sourceId: `azure:pipeline:${base}/${name}/${key}`,
					source: "azure",
					kind: "pipeline",
					priority: "urgent",
					text: `Pipeline red — ${name}/${r.definition?.name ?? "pipeline"} on ${short}`,
					url: `${base}/${encodeURIComponent(name)}/_build/results?buildId=${r.id}`,
				});
			}
		}
	}
	return { events, observed, failures };
}
