import { afterEach, expect, test } from "bun:test";
import { chmodSync, writeFileSync } from "node:fs";
import { delimiter, join } from "node:path";
import { pathToFileURL } from "node:url";
import { EventQueue, type NewEvent } from "../../src/shared/events/queue.ts";
import { AZURE_KINDS, pollAzure } from "../../src/shared/pollers/azure.ts";
import { meetingLink, pollCalendar } from "../../src/shared/pollers/calendar.ts";
import { type RunCli, runCli } from "../../src/shared/pollers/cli.ts";
import { GITHUB_KINDS, pollGitHub } from "../../src/shared/pollers/github.ts";
import { type PollerSettings, readPoller } from "../../src/shared/pollers/settings.ts";
import { openCatalog } from "../../src/shared/schema.ts";
import { useTempDirs, useTempJocasta } from "../shared/fixture.ts";

useTempJocasta();

/** A fake `gh`/`az`: answers by the command's words before its first flag or number, and logs each call. */
function fakeRun(responses: Record<string, unknown>): { run: RunCli; calls: string[][] } {
	const calls: string[][] = [];
	const run: RunCli = async (command, args) => {
		calls.push([command, ...args]);
		const words: string[] = [command];
		for (const a of args) {
			if (a.startsWith("-") || /^\d/.test(a)) break;
			words.push(a);
		}
		const key = words.join(" ");
		if (!(key in responses)) throw new Error(`no fake response for ${key}`);
		const out = responses[key];
		return typeof out === "string" ? out : JSON.stringify(out);
	};
	return { run, calls };
}

/** A PR in the GraphQL answer's `recent` list, with its last commit's checks. */
function recentPr(number: number, title: string, contexts: unknown[]): unknown {
	return {
		number,
		title,
		url: `https://github.com/acme/web/pull/${number}`,
		repository: { nameWithOwner: "acme/web" },
		commits: { nodes: [{ commit: { statusCheckRollup: { contexts: { nodes: contexts } } } }] },
	};
}

/** The GraphQL answer: his open PRs by number, and the newest of them with their checks. */
function prsAnswer(open: number[], recent: unknown[]): unknown {
	return {
		data: {
			open: { nodes: open.map((n) => ({ url: `https://github.com/acme/web/pull/${n}` })) },
			recent: { nodes: recent },
		},
	};
}

const failedTest = {
	name: "test",
	status: "COMPLETED",
	conclusion: "FAILURE",
	detailsUrl: "https://github.com/acme/web/actions/runs/1",
	startedAt: "2026-10-08T09:00:00Z",
	checkSuite: { workflowRun: { event: "pull_request", workflow: { name: "ci" } } },
};

test("GitHub: reviews requested on every notifications page are normal; failing checks on his open PRs are urgent (0301, 0298, 0303, PERF-8)", async () => {
	const { run, calls } = fakeRun({
		// `--paginate --slurp`: one array per page.
		"gh api notifications": [
			[
				{
					id: "1",
					reason: "review_requested",
					subject: { title: "Add export", url: "https://api.github.com/repos/acme/api/pulls/42", type: "PullRequest" },
					repository: { full_name: "acme/api" },
				},
			],
			[
				{ id: "2", reason: "mention", subject: { title: "x", type: "Issue" }, repository: { full_name: "acme/api" } },
				{
					id: "3",
					reason: "review_requested",
					subject: { title: "Old one", url: "https://api.github.com/repos/acme/api/pulls/9", type: "PullRequest" },
					repository: { full_name: "acme/api" },
				},
			],
		],
		"gh api graphql": prsAnswer(
			[7, 8, 10],
			[
				recentPr(7, "Fix login", [failedTest, { name: "lint", status: "COMPLETED", conclusion: "SUCCESS" }]),
				// A commit status in error counts as failing, as in `gh pr checks`.
				recentPr(8, "Docs", [{ context: "ci/build", state: "ERROR", targetUrl: "https://ci.example/b/2" }]),
				recentPr(10, "Green", [
					// A re-run replaced the failed run; cancelled, pending and startup failures aren't `fail` either.
					{ ...failedTest, startedAt: "2026-10-08T08:00:00Z" },
					{ ...failedTest, conclusion: "SUCCESS" },
					{ name: "deploy", status: "COMPLETED", conclusion: "CANCELLED" },
					{ name: "e2e", status: "IN_PROGRESS", conclusion: null },
					{ name: "boot", status: "COMPLETED", conclusion: "STARTUP_FAILURE" },
				]),
			],
		),
	});
	const { events, observed, failures } = await pollGitHub(run);
	expect(events).toEqual([
		{
			sourceId: "github:review:acme/api#42",
			source: "github",
			kind: "review",
			priority: "normal",
			text: "Review requested — Add export (acme/api #42)",
			url: "https://github.com/acme/api/pull/42",
		},
		{
			sourceId: "github:review:acme/api#9",
			source: "github",
			kind: "review",
			priority: "normal",
			text: "Review requested — Old one (acme/api #9)",
			url: "https://github.com/acme/api/pull/9",
		},
		{
			sourceId: "github:pipeline:acme/web#7",
			source: "github",
			kind: "pipeline",
			priority: "urgent",
			text: "Checks failing — acme/web #7 Fix login: test",
			url: "https://github.com/acme/web/actions/runs/1",
		},
		{
			sourceId: "github:pipeline:acme/web#8",
			source: "github",
			kind: "pipeline",
			priority: "urgent",
			text: "Checks failing — acme/web #8 Docs: ci/build",
			url: "https://ci.example/b/2",
		},
	]);
	expect(failures).toEqual([]);
	// Two processes a round: every notifications page, then one GraphQL search for his open PRs and their checks.
	expect(calls.map((c) => c.slice(0, 3).join(" "))).toEqual(["gh api notifications", "gh api graphql"]);
	expect(calls[0]).toEqual(["gh", "api", "notifications", "--paginate", "--slurp"]);
	expect(calls[1]?.some((a) => a.startsWith("q=") && a.includes("author:@me") && a.includes("is:open"))).toBe(true);
	expect(calls[1]?.some((a) => a.startsWith("query=") && a.includes("first: 100)") && a.includes("first: 10)"))).toBe(
		true,
	);
	// Reviews are all seen; a pipeline only when its PR's checks were read or it is no longer open (SHR-8).
	expect(observed("github:review:acme/api#1")).toBe(true);
	expect(observed("github:pipeline:acme/web#10")).toBe(true);
	expect(observed("github:pipeline:acme/web#99")).toBe(true);
});

test("GitHub: a failing PR beyond the newest 10 stays open; one merged or gone green is resolved (SHR-8)", async () => {
	const db = openCatalog();
	const queue = new EventQueue(db);
	try {
		const pipeline = (n: number): NewEvent => ({
			sourceId: `github:pipeline:acme/web#${n}`,
			source: "github",
			kind: "pipeline",
			priority: "urgent",
			text: `Checks failing — acme/web #${n}`,
		});
		for (const n of [3, 4, 5]) queue.add(pipeline(n));
		// #3 is still open but not among the newest 10 (not checked); #4 was checked and is green; #5 is no longer open.
		const { run } = fakeRun({
			"gh api notifications": [],
			"gh api graphql": prsAnswer([1, 3, 4], [recentPr(1, "New", []), recentPr(4, "Green now", [])]),
		});
		const round = await pollGitHub(run);
		queue.sync("github", GITHUB_KINDS, round.events, round.observed);
		expect(queue.open().map((r) => r.source_id)).toEqual(["github:pipeline:acme/web#3"]);
	} finally {
		db.close();
	}
});

test("Azure DevOps: unvoted reviews per org; the newest run of a pipeline red on main or his branch (0301, 0303)", async () => {
	const org = "https://dev.azure.com/example-org";
	const { run, calls } = fakeRun({
		"az account show": "him@example.test\n",
		"az repos pr list": [
			{
				pullRequestId: 5,
				title: "Export v2",
				repository: { name: "api", project: { name: "Client A" } },
				reviewers: [{ uniqueName: "Him@example.test", vote: 0 }],
			},
			{
				pullRequestId: 6,
				title: "Approved already",
				repository: { name: "api", project: { name: "Client A" } },
				reviewers: [{ uniqueName: "him@example.test", vote: 10 }],
			},
		],
		"az devops project list": { value: [{ name: "Client A" }] },
		"az pipelines runs list": [
			// Newest first: main is red now; an older green main run doesn't hide it.
			{
				id: 30,
				status: "completed",
				result: "failed",
				sourceBranch: "refs/heads/main",
				definition: { id: 1, name: "web" },
			},
			{
				id: 29,
				status: "completed",
				result: "succeeded",
				sourceBranch: "refs/heads/main",
				definition: { id: 1, name: "web" },
			},
			// His branch went green again: not red.
			{
				id: 28,
				status: "completed",
				result: "succeeded",
				sourceBranch: "refs/heads/him/x",
				definition: { id: 1 },
				requestedFor: { uniqueName: "him@example.test" },
			},
			{
				id: 27,
				status: "completed",
				result: "failed",
				sourceBranch: "refs/heads/him/x",
				definition: { id: 1 },
				requestedFor: { uniqueName: "him@example.test" },
			},
			// Someone else's branch: not his business.
			{
				id: 26,
				status: "completed",
				result: "failed",
				sourceBranch: "refs/heads/other/y",
				definition: { id: 1 },
				requestedFor: { uniqueName: "else@example.test" },
			},
			{ id: 25, status: "inProgress", sourceBranch: "refs/heads/main", definition: { id: 2 } },
		],
	});
	expect((await pollAzure(run, [])).events).toEqual([]);
	expect(calls).toEqual([]);
	const { events, failures } = await pollAzure(run, [`${org}/`]);
	expect(failures).toEqual([]);
	expect(events).toEqual([
		{
			sourceId: `azure:review:${org}/Client A/api#5`,
			source: "azure",
			kind: "review",
			priority: "normal",
			text: "Review requested — Export v2 (Client A/api !5)",
			url: `${org}/Client%20A/_git/api/pullrequest/5`,
		},
		{
			sourceId: `azure:pipeline:${org}/Client A/1:refs/heads/main`,
			source: "azure",
			kind: "pipeline",
			priority: "urgent",
			text: "Pipeline red — Client A/web on main",
			url: `${org}/Client%20A/_build/results?buildId=30`,
		},
	]);
	expect(calls.find((c) => c[1] === "repos")).toEqual([
		"az",
		"repos",
		"pr",
		"list",
		"--organization",
		org,
		"--reviewer",
		"him@example.test",
		"--status",
		"active",
		"--output",
		"json",
	]);
});

test("Azure DevOps: an org or project whose listing fails keeps its events open, the rest is read, the failure reported (SHR-8)", async () => {
	const good = "https://dev.azure.com/example-org";
	const bad = "https://dev.azure.com/acme";
	const run: RunCli = async (_command, args) => {
		const org = args[args.indexOf("--organization") + 1];
		const project = args.includes("--project") ? args[args.indexOf("--project") + 1] : undefined;
		if (args[0] === "account") return "him@example.test\n";
		if (org === bad && args[0] === "repos") throw new Error("az repos pr failed: TF400813: not authorized");
		if (args[0] === "repos") return "[]";
		if (args[0] === "devops") {
			if (org === bad) throw new Error("az devops project failed: TF400813: not authorized");
			return JSON.stringify({ value: [{ name: "Client A" }, { name: "Client B" }] });
		}
		if (project === "Client A") throw new Error("az pipelines runs failed: TF401444: no access");
		return JSON.stringify([
			{ id: 3, status: "completed", result: "failed", sourceBranch: "refs/heads/main", definition: { id: 1 } },
		]);
	};
	const db = openCatalog();
	const queue = new EventQueue(db);
	try {
		const event = (sourceId: string, kind: string): NewEvent => ({
			sourceId,
			source: "azure",
			kind,
			priority: "normal",
			text: sourceId,
		});
		const kept = [
			event(`azure:review:${bad}/Web/site#1`, "review"),
			event(`azure:pipeline:${bad}/Web/1:refs/heads/main`, "pipeline"),
			event(`azure:pipeline:${good}/Client A/1:refs/heads/main`, "pipeline"),
		];
		const gone = [
			event(`azure:review:${good}/Client A/api#2`, "review"),
			event(`azure:pipeline:${good}/Client B/2:refs/heads/main`, "pipeline"),
		];
		for (const e of [...kept, ...gone]) queue.add(e);
		const round = await pollAzure(run, [good, bad]);
		expect(round.events.map((e) => e.sourceId)).toEqual([`azure:pipeline:${good}/Client B/1:refs/heads/main`]);
		expect(round.failures).toEqual([
			"az pipelines runs failed: TF401444: no access",
			"az repos pr failed: TF400813: not authorized",
			"az devops project failed: TF400813: not authorized",
		]);
		queue.sync("azure", AZURE_KINDS, round.events, round.observed);
		expect(queue.open().map((r) => r.source_id)).toEqual([
			...kept.map((e) => e.sourceId),
			`azure:pipeline:${good}/Client B/1:refs/heads/main`,
		]);
	} finally {
		db.close();
	}
});

test("poller settings: 0301's and 0442's defaults, and a malformed value names its key and turns off only its poller", () => {
	const all = (settings: Record<string, unknown>) => ({
		github: readPoller(settings, "github"),
		azureDevOps: readPoller(settings, "azureDevOps"),
		calendar: readPoller(settings, "calendar"),
		news: readPoller(settings, "news"),
	});
	const news = { feeds: [{ source: "hn" }, { source: "lwn" }, { topic: "gaming", limit: 10 }], everyMinutes: 15 };
	expect(all({})).toEqual({
		github: { everyMinutes: 2 },
		azureDevOps: { orgs: [], everyMinutes: 5 },
		calendar: { everyMinutes: 10 },
		news,
	} satisfies PollerSettings);
	expect(
		all({
			pollers: {
				azureDevOps: { orgs: ["https://dev.azure.com/example-org"] },
				github: { everyMinutes: 3 },
				news: { feeds: [{ topic: "science", limit: 5 }, { source: "acme" }] },
			},
		}),
	).toEqual({
		github: { everyMinutes: 3 },
		azureDevOps: { orgs: ["https://dev.azure.com/example-org"], everyMinutes: 5 },
		calendar: { everyMinutes: 10 },
		news: { feeds: [{ topic: "science", limit: 5 }, { source: "acme" }], everyMinutes: 15 },
	});
	expect(readPoller({ pollers: { news: { feeds: [] } } }, "news").feeds).toEqual([]);
	// A bad GitHub value turns off GitHub only; the other pollers read their own keys.
	const badGitHub = { pollers: { github: { everyMinutes: 0 }, calendar: { everyMinutes: 20 } } };
	expect(() => readPoller(badGitHub, "github")).toThrow("pollers.github.everyMinutes");
	expect(readPoller(badGitHub, "calendar")).toEqual({ everyMinutes: 20 });
	expect(readPoller(badGitHub, "news")).toEqual(news);
	// A `pollers` that isn't a mapping turns them all off.
	expect(() => readPoller({ pollers: 5 }, "calendar")).toThrow("settings: pollers must be a mapping");
	expect(() => readPoller({ pollers: { azureDevOps: { orgs: ["example-org"] } } }, "azureDevOps")).toThrow(
		"pollers.azureDevOps.orgs",
	);
	const shape = "pollers.news.feeds must be a list of {source: <name>} or {topic: <name>}";
	for (const feeds of [
		{ source: "hn" },
		[{ source: "hn", topic: "gaming" }],
		[{ limit: 5 }],
		[{ source: "Hacker News" }],
		[{ topic: "gaming", limit: 201 }],
		[{ topic: "gaming", limit: 2.5 }],
	]) {
		expect(() => readPoller({ pollers: { news: { feeds } } }, "news")).toThrow(shape);
	}
	const gone = "settings: pollers.news.topics and pollers.news.limit are gone; use pollers.news.feeds (0442)";
	expect(() => readPoller({ pollers: { news: { topics: ["tech"] } } }, "news")).toThrow(gone);
	expect(() => readPoller({ pollers: { news: { limit: 10 } } }, "news")).toThrow(gone);
	expect(() => readPoller({ pollers: { news: { everyMinutes: 0 } } }, "news")).toThrow("pollers.news.everyMinutes");
});

const fakeDir = useTempDirs("jocasta fake gh ");
const savedPath = process.env.PATH;
afterEach(() => {
	process.env.PATH = savedPath;
});

/** A folder with spaces holding a fake `gh` that prints its arguments as JSON; "fail", "hang" and "checks" act. */
function fakeGh(): string {
	const dir = fakeDir();
	writeFileSync(
		join(dir, "fake.ts"),
		`const args = process.argv.slice(2);
if (args[0] === "fail") { console.error("boom\\nmore"); process.exit(2); }
if (args[0] === "hang") await Bun.sleep(10_000);
console.log(JSON.stringify(args));
process.exit(args[0] === "checks" ? 1 : 0);
`,
	);
	if (process.platform === "win32") {
		writeFileSync(join(dir, "gh.cmd"), `@"${process.execPath}" "%~dp0fake.ts" %*\r\n`);
	} else {
		writeFileSync(join(dir, "gh"), `#!/bin/sh\nexec "${process.execPath}" "$(dirname "$0")/fake.ts" "$@"\n`);
		chmodSync(join(dir, "gh"), 0o755);
	}
	return dir;
}

test("runCli runs the gh on PATH, in a dir with spaces, and accepts the exit codes a caller allows", async () => {
	const dir = fakeGh();
	process.env.PATH = `${dir}${delimiter}${savedPath ?? ""}`;
	expect(JSON.parse(await runCli("gh", ["api", "a b"]))).toEqual(["api", "a b"]);
	expect(JSON.parse(await runCli("gh", ["checks"], [1]))).toEqual(["checks"]);
	await expect(runCli("gh", ["checks"])).rejects.toThrow("gh checks failed: exit 1");
	await expect(runCli("gh", ["fail"])).rejects.toThrow("gh fail failed: boom");
	await expect(runCli("gh", ['say "hi"'])).rejects.toThrow('holds " or %');
	process.env.PATH = dir;
	await expect(runCli("az", ["x"])).rejects.toThrow("az is not on PATH");
	process.env.PATH = `${dir}${delimiter}${savedPath ?? ""}`;
	// The stop's abort ends a command that would answer only after the test's 5 s timeout; on Windows the fake is a
	// `.cmd`, and its child keeps the pipes after cmd.exe is killed (and ends on its own).
	const stop = new AbortController();
	const hanging = runCli("gh", ["hang"], [], stop.signal);
	stop.abort();
	await expect(hanging).rejects.toThrow();
});

test("runCli finds gh from a Worker whose environment spells the variable Path, like the companion's", async () => {
	// Electrobun runs the companion in `new Worker(entry, {})`: its `process.env` is a case-sensitive copy, and Windows
	// spells the variable `Path` (a shell such as Git Bash passes `PATH`).
	const dir = fakeGh();
	const entry = join(dir, "worker.ts");
	const cli = pathToFileURL(join(import.meta.dir, "../../src/shared/pollers/cli.ts")).href;
	writeFileSync(
		entry,
		`import { runCli } from ${JSON.stringify(cli)};
runCli("gh", ["api", "x"]).then(postMessage, (error) => postMessage(String(error)));
`,
	);
	const env = Object.fromEntries(
		Object.entries(process.env).filter(([key, value]) => value !== undefined && key.toUpperCase() !== "PATH"),
	) as Record<string, string>;
	const worker = new Worker(entry, { env: { ...env, Path: `${dir}${delimiter}${savedPath ?? ""}` } });
	const answer = Promise.withResolvers<string>();
	worker.onmessage = (event) => answer.resolve(String(event.data));
	worker.onerror = (event) => answer.resolve(`worker failed: ${event.message}`);
	try {
		expect((await answer.promise).trim()).toBe(JSON.stringify(["api", "x"]));
	} finally {
		worker.terminate();
	}
});

/**
 * A local iCal feed: a weekly standup with one day skipped and one moved, a cancelled one-off, one occurrence of a
 * series whose master isn't in the feed, an all-day event.
 */
const FEED = [
	"BEGIN:VCALENDAR",
	"VERSION:2.0",
	"PRODID:-//test//EN",
	"BEGIN:VEVENT",
	"UID:standup@test",
	"SUMMARY:standup (client-a)",
	"DTSTART:20261001T080000Z",
	"DTEND:20261001T081500Z",
	"RRULE:FREQ=DAILY",
	"EXDATE:20261009T080000Z",
	"END:VEVENT",
	"BEGIN:VEVENT",
	"UID:standup@test",
	"RECURRENCE-ID:20261008T080000Z",
	"SUMMARY:standup (moved)",
	"DTSTART:20261008T090000Z",
	"DTEND:20261008T091500Z",
	"END:VEVENT",
	"BEGIN:VEVENT",
	"UID:cancelled@test",
	"SUMMARY:dropped",
	"STATUS:CANCELLED",
	"DTSTART:20261008T120000Z",
	"DTEND:20261008T130000Z",
	"END:VEVENT",
	"BEGIN:VEVENT",
	"UID:weekly-sync@client-a.example",
	"RECURRENCE-ID:20261008T100000Z",
	"SUMMARY:weekly sync (one occurrence)",
	"DTSTART:20261008T103000Z",
	"DTEND:20261008T110000Z",
	"END:VEVENT",
	"BEGIN:VEVENT",
	"UID:allday@test",
	"SUMMARY:conference",
	"DTSTART;VALUE=DATE:20261008",
	"DTEND;VALUE=DATE:20261009",
	"END:VEVENT",
	"BEGIN:VEVENT",
	"UID:later@test",
	"SUMMARY:next week",
	"DTSTART:20261015T100000Z",
	"DTEND:20261015T110000Z",
	"END:VEVENT",
	"END:VCALENDAR",
	"",
].join("\r\n");

test("Calendar: the secret iCal URL is read over HTTP, today and tomorrow expanded with exceptions; failures never show the URL (0301, 0353)", async () => {
	using server = Bun.serve({
		port: 0,
		fetch: (request) =>
			new URL(request.url).pathname === "/private-abc123/basic.ics"
				? new Response(FEED, { headers: { "content-type": "text/calendar" } })
				: new Response("no", { status: 404 }),
	});
	const url = `http://127.0.0.1:${server.port}/private-abc123/basic.ics`;
	const meetings = await pollCalendar(url, new Date("2026-10-08T06:00:00Z"));
	const seen = meetings.map((m) => `${m.start.toISOString()} ${m.title}${m.allDay ? " (all day)" : ""}`);
	expect(seen).toContain("2026-10-08T09:00:00.000Z standup (moved)");
	expect(seen.some((s) => s.startsWith("2026-10-08T08:00"))).toBe(false);
	expect(seen.some((s) => s.includes("2026-10-09T08:00"))).toBe(false);
	expect(seen.some((s) => s.includes("dropped") || s.includes("next week"))).toBe(false);
	expect(seen.some((s) => s.endsWith("conference (all day)"))).toBe(true);
	// One occurrence of a series whose master isn't in the feed shows as a meeting of its own (SHR-4).
	expect(seen).toContain("2026-10-08T10:30:00.000Z weekly sync (one occurrence)");
	// Each meeting carries its end, for today's calendar (C9): an override's own, a lone occurrence's own.
	const ends = (list: typeof meetings) =>
		list.filter((m) => !m.allDay).map((m) => `${m.title} ${m.start.toISOString()}-${m.end.toISOString()}`);
	expect(ends(meetings)).toEqual([
		"standup (moved) 2026-10-08T09:00:00.000Z-2026-10-08T09:15:00.000Z",
		"weekly sync (one occurrence) 2026-10-08T10:30:00.000Z-2026-10-08T11:00:00.000Z",
	]);
	// A series' occurrence ends at its start plus the event's duration.
	expect(ends(await pollCalendar(url, new Date("2026-10-02T06:00:00Z")))).toEqual([
		"standup (client-a) 2026-10-02T08:00:00.000Z-2026-10-02T08:15:00.000Z",
		"standup (client-a) 2026-10-03T08:00:00.000Z-2026-10-03T08:15:00.000Z",
	]);

	const missing = `http://127.0.0.1:${server.port}/private-secret-xyz/none.ics`;
	const error = await pollCalendar(missing, new Date()).catch((e: Error) => e);
	expect((error as Error).message).toBe("calendar: HTTP 404");
	const unreachable = await pollCalendar("http://127.0.0.1:1/private-secret-xyz", new Date()).catch((e: Error) => e);
	expect((unreachable as Error).message).toBe("calendar: it couldn't be fetched");
	expect((unreachable as Error).message).not.toContain("private-secret-xyz");
});

test("Calendar: a webcal:// subscription link is fetched over https (D7)", async () => {
	const fetched: string[] = [];
	const fetchUrl = (async (input: string | URL | Request) => {
		fetched.push(String(input));
		return new Response(FEED, { headers: { "content-type": "text/calendar" } });
	}) as typeof fetch;
	await pollCalendar("webcal://calendar.example/x.ics", new Date("2026-10-08T06:00:00Z"), fetchUrl);
	expect(fetched).toEqual(["https://calendar.example/x.ics"]);
});

test("links from a calendar or a CI check are kept only as http(s) links up to 300 characters (SE-4, SE-6)", async () => {
	expect(meetingLink({ url: "javascript:alert(1)" } as Parameters<typeof meetingLink>[0])).toBeUndefined();
	expect(meetingLink({ url: "HTTPS://Meet.Example/abc" } as Parameters<typeof meetingLink>[0])).toBe(
		"https://meet.example/abc",
	);
	expect(
		meetingLink({ location: `https://meet.example/${"x".repeat(300)}` } as Parameters<typeof meetingLink>[0]),
	).toBeUndefined();
	const { run } = fakeRun({
		"gh api notifications": [],
		"gh api graphql": prsAnswer([7], [recentPr(7, "Fix", [{ ...failedTest, detailsUrl: "javascript:alert(1)" }])]),
	});
	expect((await pollGitHub(run)).events.map((e) => e.url)).toEqual(["https://github.com/acme/web/pull/7"]);
});
