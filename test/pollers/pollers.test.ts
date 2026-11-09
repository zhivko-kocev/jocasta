import { afterEach, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { pathToFileURL } from "node:url";
import { pollAzure } from "../../src/shared/pollers/azure.ts";
import { meetingLink, pollCalendar } from "../../src/shared/pollers/calendar.ts";
import { type RunCli, runCli } from "../../src/shared/pollers/cli.ts";
import { pollGitHub } from "../../src/shared/pollers/github.ts";
import { readPollerSettings } from "../../src/shared/pollers/settings.ts";

/** A fake `gh`/`az`: answers by the command's words before its first flag or number, and logs each call. */
function fakeRun(
	responses: Record<string, unknown>,
	codes: Record<string, number> = {},
): {
	run: RunCli;
	calls: string[][];
} {
	const calls: string[][] = [];
	const run: RunCli = async (command, args, okCodes = []) => {
		calls.push([command, ...args]);
		const words: string[] = [command];
		for (const a of args) {
			if (a.startsWith("-") || /^\d/.test(a)) break;
			words.push(a);
		}
		const key = words.join(" ");
		const code = codes[key] ?? 0;
		if (code !== 0 && !okCodes.includes(code)) throw new Error(`${key} failed: exit ${code}`);
		if (!(key in responses)) throw new Error(`no fake response for ${key}`);
		const out = responses[key];
		return typeof out === "string" ? out : JSON.stringify(out);
	};
	return { run, calls };
}

test("GitHub: reviews requested are normal; failing checks on his open PRs are urgent (0301, 0298, 0303)", async () => {
	const { run, calls } = fakeRun(
		{
			"gh api notifications": [
				{
					id: "1",
					reason: "review_requested",
					subject: { title: "Add export", url: "https://api.github.com/repos/acme/api/pulls/42", type: "PullRequest" },
					repository: { full_name: "acme/api" },
				},
				{ id: "2", reason: "mention", subject: { title: "x", type: "Issue" }, repository: { full_name: "acme/api" } },
			],
			"gh search prs": [
				{
					number: 7,
					title: "Fix login",
					url: "https://github.com/acme/web/pull/7",
					repository: { nameWithOwner: "acme/web" },
				},
				{
					number: 8,
					title: "Docs",
					url: "https://github.com/acme/web/pull/8",
					repository: { nameWithOwner: "acme/web" },
				},
			],
			"gh pr checks": [
				{ name: "test", bucket: "fail", link: "https://github.com/acme/web/actions/runs/1" },
				{ name: "lint", bucket: "pass" },
			],
		},
		// `gh pr checks` exits 1 when a check failed; its JSON still counts.
		{ "gh pr checks": 1 },
	);
	const events = await pollGitHub(run);
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
			text: "Checks failing — acme/web #8 Docs: test",
			url: "https://github.com/acme/web/actions/runs/1",
		},
	]);
	expect(calls.map((c) => c.slice(0, 3).join(" "))).toEqual([
		"gh api notifications",
		"gh search prs",
		"gh pr checks",
		"gh pr checks",
	]);
	expect(calls[1]).toContain("@me");
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
	expect(await pollAzure(run, [])).toEqual([]);
	expect(calls).toEqual([]);
	const events = await pollAzure(run, [`${org}/`]);
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

test("poller settings: 0301's and 0442's defaults, and a malformed value names its key", () => {
	const news = { feeds: [{ source: "hn" }, { source: "lwn" }, { topic: "gaming", limit: 10 }], everyMinutes: 15 };
	expect(readPollerSettings({})).toEqual({
		github: { everyMinutes: 2 },
		azureDevOps: { orgs: [], everyMinutes: 5 },
		calendar: { everyMinutes: 10 },
		news,
	});
	expect(
		readPollerSettings({
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
	expect(readPollerSettings({ pollers: { news: { feeds: [] } } }).news.feeds).toEqual([]);
	expect(() => readPollerSettings({ pollers: { github: { everyMinutes: 0 } } })).toThrow("pollers.github.everyMinutes");
	expect(() => readPollerSettings({ pollers: { azureDevOps: { orgs: ["example-org"] } } })).toThrow(
		"pollers.azureDevOps.orgs",
	);
	const shape = "pollers.news.feeds must be a list of {source: <name>} or {topic: <name>}";
	expect(() => readPollerSettings({ pollers: { news: { feeds: { source: "hn" } } } })).toThrow(shape);
	expect(() => readPollerSettings({ pollers: { news: { feeds: [{ source: "hn", topic: "gaming" }] } } })).toThrow(
		shape,
	);
	expect(() => readPollerSettings({ pollers: { news: { feeds: [{ limit: 5 }] } } })).toThrow(shape);
	expect(() => readPollerSettings({ pollers: { news: { feeds: [{ source: "Hacker News" }] } } })).toThrow(shape);
	expect(() => readPollerSettings({ pollers: { news: { feeds: [{ topic: "gaming", limit: 201 }] } } })).toThrow(shape);
	expect(() => readPollerSettings({ pollers: { news: { feeds: [{ topic: "gaming", limit: 2.5 }] } } })).toThrow(shape);
	const gone = "settings: pollers.news.topics and pollers.news.limit are gone; use pollers.news.feeds (0442)";
	expect(() => readPollerSettings({ pollers: { news: { topics: ["tech"] } } })).toThrow(gone);
	expect(() => readPollerSettings({ pollers: { news: { limit: 10 } } })).toThrow(gone);
	expect(() => readPollerSettings({ pollers: { news: { everyMinutes: 0 } } })).toThrow("pollers.news.everyMinutes");
});

const fakeDirs: string[] = [];
const savedPath = process.env.PATH;
afterEach(() => {
	process.env.PATH = savedPath;
	for (const dir of fakeDirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

/** A folder with spaces holding a fake `gh` that prints its arguments as JSON; "fail", "hang" and "checks" act. */
function fakeGh(): string {
	const dir = mkdtempSync(join(tmpdir(), "jocasta fake gh "));
	fakeDirs.push(dir);
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

/** A local iCal feed: a weekly standup with one day skipped and one moved, a cancelled one-off, an all-day event. */
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
	const { run } = fakeRun(
		{
			"gh api notifications": [],
			"gh search prs": [
				{
					number: 7,
					title: "Fix",
					url: "https://github.com/acme/web/pull/7",
					repository: { nameWithOwner: "acme/web" },
				},
			],
			"gh pr checks": [{ name: "test", bucket: "fail", link: "javascript:alert(1)" }],
		},
		{ "gh pr checks": 1 },
	);
	expect((await pollGitHub(run)).map((e) => e.url)).toEqual(["https://github.com/acme/web/pull/7"]);
});
