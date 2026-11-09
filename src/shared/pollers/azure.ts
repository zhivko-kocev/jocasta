import type { NewEvent } from "../events/queue.ts";
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
 * (urgent, 0303).
 */
export async function pollAzure(run: RunCli, orgs: readonly string[]): Promise<NewEvent[]> {
	if (orgs.length === 0) return [];
	const user = await me(run);
	const events: NewEvent[] = [];
	for (const org of orgs) {
		const base = org.replace(/\/+$/, "");
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
		const projects = await runJson<{ value?: { name: string }[] }>(
			run,
			"az",
			["devops", "project", "list", "--organization", base, "--output", "json"],
			{},
		);
		for (const { name } of projects.value ?? []) {
			if (/["%]/.test(name)) continue;
			const runs = await runJson<AzRun[]>(
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
	return events;
}
