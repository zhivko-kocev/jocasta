// A bad value in jocasta.yaml turns off only its own feature (0584): one bad poller key stops that poller alone, the
// other pollers and the weather run, and the problem is said once (the service's notification at start).
import { expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Day } from "../../../src/frontier/server/day.ts";
import type { RunCli } from "../../../src/shared/pollers/cli.ts";
import { openCatalog } from "../../../src/shared/schema.ts";
import { settingsProblems } from "../../../src/shared/settings.ts";
import { useTempJocasta } from "../../shared/fixture.ts";

const env = useTempJocasta();

test("a bad pollers.github value turns off GitHub only: Azure DevOps, the calendar, news and weather run; said once", async () => {
	mkdirSync(env.config, { recursive: true });
	writeFileSync(
		join(env.config, "jocasta.yaml"),
		[
			"dashboard:",
			"  weather: Example City",
			"pollers:",
			"  github:",
			"    everyMinutes: 0",
			"  azureDevOps:",
			"    orgs: [https://dev.azure.com/example-org]",
			"",
		].join("\n"),
	);
	const notes: string[] = [];
	// The service says each settings problem once at start, before the day starts (C7).
	for (const problem of settingsProblems()) notes.push(`settings: ${problem}`);

	const commands: string[] = [];
	const azure = Promise.withResolvers<void>();
	const run: RunCli = async (command) => {
		commands.push(command);
		if (command === "az") azure.resolve();
		throw new Error(`${command} isn't here`);
	};
	const fetched = { news: Promise.withResolvers<void>(), weather: Promise.withResolvers<void>() };
	const calendar = Promise.withResolvers<void>();
	const fetchUrl = Object.assign(
		async (input: string | URL | Request) => {
			const url = String(input);
			if (url.includes("brutalist")) fetched.news.resolve();
			else if (url.includes("calendar.example")) calendar.resolve();
			else fetched.weather.resolve();
			return new Response("not here", { status: 503 });
		},
		{ preconnect: fetch.preconnect },
	) as typeof fetch;

	const catalog = openCatalog();
	const day = new Day(
		catalog,
		{
			send: () => {},
			alert: () => {},
			notify: (text) => notes.push(text),
			projects: () => [],
			threads: () => [],
			sessionsByProject: () => new Map(),
			prepareSessions: async () => {},
			services: async () => [],
			projectName: () => null,
			leftovers: async () => [],
			weeklyReview: async () => null,
		},
		{
			run,
			fetchUrl,
			calendarSecret: () => "https://calendar.example/his.ics",
			usage: async () => {
				throw new Error("no omp here");
			},
			diskFree: () => 0.5,
		},
	);
	try {
		day.start();
		await Promise.all([azure.promise, calendar.promise, fetched.news.promise, fetched.weather.promise]);
	} finally {
		await day.stop();
		catalog.close();
	}
	expect(commands).not.toContain("gh");
	expect(commands).toContain("az");
	expect(notes.filter((note) => note.includes("pollers.github"))).toEqual([
		"settings: jocasta.yaml:5: pollers.github.everyMinutes must be a number of minutes, 1 or more; the GitHub poller is off",
	]);
});
