// H7 (0482): long-path robustness of each tool's default worktree location, with Git's default settings.
//
//   bun bench/h7-longpath.ts --out <dir> [--root <dir>] [--lengths 100,110,…,240] [--runs 1]
//                            [--home <dir>] [--location name=<template>] [--dry-run]
//
// For each L it builds a fixture repo `<root>/acme-app` whose deepest file has a relative path of exactly L characters,
// then checks a branch out at each location with `git worktree add` and runs `git status` there. Git runs with no
// global or system config (`GIT_CONFIG_GLOBAL` an empty file, `GIT_CONFIG_NOSYSTEM=1`), so `core.longpaths` is Git's
// default. Locations (`<repo>` the main checkout, `<name>` its folder name, `<slug>` the branch):
//   jocasta      <parent>/<name>.wt/<slug>              (`service.ts` worktree path, CHANGELOG 0457)
//   herdr        <home>/.herdr/worktrees/<name>/<slug>  (herdr's default)
//   claude       <repo>/.claude/worktrees/<slug>        (Claude Code `--worktree`; confirm in the VM)
// `--location name=<template>` adds one (vibe-kanban's folder once confirmed there). `--home` defaults to a temp
// folder, so nothing lands in his home. Writes h7-longpath.jsonl and h7-longpath.json (the largest passing L).
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { removeTree } from "../src/shared/fsx.ts";
import { runProgramSync } from "../src/shared/process.ts";
import { BenchUsageError, flag, numberListFlag, parseBenchArgs, runScript } from "./lib/args.ts";
import { appendRow, claimRows, conditions, printPlan, writeJson } from "./lib/output.ts";

export const DEFAULT_LOCATIONS: Record<string, string> = {
	jocasta: "<parent>/<name>.wt/<slug>",
	herdr: "<home>/.herdr/worktrees/<name>/<slug>",
	claude: "<repo>/.claude/worktrees/<slug>",
};

export const DEFAULT_LENGTHS = [100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240];

/** A relative path (with `/`) of exactly `length` characters: folders of up to 20 characters, then `file.txt`. */
export function deepPath(length: number): string {
	if (length < 10) throw new BenchUsageError("a path length below 10 can't hold a folder and the file");
	const file = "file.txt";
	const parts: string[] = [];
	let left = length - file.length;
	let i = 0;
	while (left > 0) {
		// Each folder adds its name and one "/". A last remainder of 1 can't hold a name and "/", so it borrows.
		let size = Math.min(21, left);
		if (left - size === 1) size -= 1;
		const name = `d${i}`.padEnd(size - 1, "x").slice(0, size - 1);
		parts.push(name);
		left -= size;
		i++;
	}
	return [...parts, file].join("/");
}

export function locationPath(template: string, repo: string, home: string, slug: string): string {
	return template
		.replaceAll("<repo>", repo)
		.replaceAll("<parent>", dirname(repo))
		.replaceAll("<name>", basename(repo))
		.replaceAll("<home>", home)
		.replaceAll("<slug>", slug);
}

/** The largest L whose every run passed, per location; null when none did. */
export function largestPassing(
	rows: readonly { location: string; length: number; ok: boolean }[],
): Record<string, number | null> {
	const result: Record<string, number | null> = {};
	for (const location of new Set(rows.map((row) => row.location))) {
		const passing = [...new Set(rows.filter((row) => row.location === location).map((row) => row.length))].filter(
			(length) => rows.filter((row) => row.location === location && row.length === length).every((row) => row.ok),
		);
		result[location] = passing.length === 0 ? null : Math.max(...passing);
	}
	return result;
}

if (import.meta.main) {
	await runScript(async () => {
		const argv = process.argv.slice(2);
		// `--location` repeats; take them off first.
		const extra: Record<string, string> = {};
		const rest: string[] = [];
		for (let i = 0; i < argv.length; i++) {
			if (argv[i] !== "--location") {
				rest.push(argv[i] ?? "");
				continue;
			}
			const spec = argv[++i] ?? "";
			const at = spec.indexOf("=");
			if (at < 1) throw new BenchUsageError(`--location takes name=<template>, not "${spec}"`);
			extra[spec.slice(0, at)] = spec.slice(at + 1);
		}
		const args = parseBenchArgs(rest, { defaultRuns: 1, valued: ["--root", "--lengths", "--home"] });
		const lengths = numberListFlag(args, "lengths", DEFAULT_LENGTHS);
		const locations = { ...DEFAULT_LOCATIONS, ...extra };
		const rootFlag = flag(args, "root");
		const homeFlag = flag(args, "home");
		if (args.dryRun) {
			printPlan(
				"H7 long paths",
				[
					`fixture repos under ${rootFlag ?? "a temp folder"} with a deepest relative path of L = ${lengths.join(", ")}`,
					...Object.entries(locations).map(([name, template]) => `${name}: git worktree add ${template}`),
					`home for <home>: ${homeFlag ?? "a temp folder"}; Git with no global or system config`,
					`${args.runs} run(s) each; ok = worktree add and git status exit 0 and the deep file exists there`,
				],
				args.out,
			);
			return;
		}
		const out = args.out as string;
		claimRows(out, "h7-longpath");
		const started = conditions(["git"]);
		const root = rootFlag ? resolve(rootFlag) : mkdtempSync(join(tmpdir(), "h7-"));
		const home = homeFlag ? resolve(homeFlag) : mkdtempSync(join(tmpdir(), "h7-home-"));
		const emptyConfig = join(mkdtempSync(join(tmpdir(), "h7-git-")), "empty.gitconfig");
		writeFileSync(emptyConfig, "");
		const env = {
			...process.env,
			GIT_CONFIG_GLOBAL: emptyConfig,
			GIT_CONFIG_NOSYSTEM: "1",
			GIT_AUTHOR_NAME: "Bench",
			GIT_AUTHOR_EMAIL: "bench@example.org",
			GIT_COMMITTER_NAME: "Bench",
			GIT_COMMITTER_EMAIL: "bench@example.org",
		};
		const git = (cwd: string, gitArgs: string[]) => runProgramSync(["git", ...gitArgs], { cwd, env });
		const rows: {
			location: string;
			length: number;
			run: number;
			ok: boolean;
			absoluteLength: number;
			error: string | null;
		}[] = [];
		for (const length of lengths) {
			for (let run = 0; run < args.runs; run++) {
				const repo = join(root, `acme-app`);
				removeTree(repo);
				for (const template of Object.values(locations)) {
					const stale = locationPath(template, repo, home, "x");
					removeTree(dirname(stale));
				}
				mkdirSync(repo, { recursive: true });
				const deep = deepPath(length);
				mkdirSync(dirname(join(repo, deep)), { recursive: true });
				writeFileSync(join(repo, deep), "h7\n");
				const init = [
					git(repo, ["init", "-q", "-b", "main"]),
					git(repo, ["add", "-A"]),
					git(repo, ["commit", "-q", "-m", "fixture"]),
				];
				const fixtureError = init.find((result) => result.code !== 0);
				for (const [location, template] of Object.entries(locations)) {
					const slug = `h7-${length}-${run}`;
					const path = locationPath(template, repo, home, slug);
					let error: string | null = fixtureError ? `fixture: ${fixtureError.stderr.trim()}` : null;
					if (!error) {
						mkdirSync(dirname(path), { recursive: true });
						// One branch per location: the folder is the slug, the branch names its location too.
						const add = git(repo, ["worktree", "add", "-q", "-b", `${location}/${slug}`, path]);
						const status = add.code === 0 ? git(path, ["status", "--porcelain"]) : undefined;
						if (add.code !== 0) error = `worktree add: ${add.stderr.trim()}`;
						else if (status?.code !== 0) error = `status: ${status?.stderr.trim()}`;
						else if (!existsSync(join(path, deep))) error = "the deep file is missing in the worktree";
					}
					const row = {
						location,
						length,
						run,
						ok: error === null,
						absoluteLength: join(path, deep).length,
						error,
					};
					rows.push(row);
					appendRow(out, "h7-longpath", row);
				}
			}
		}
		const file = writeJson(out, "h7-longpath", {
			benchmark: "H7",
			conditions: { ...started, finishedAt: new Date().toISOString() },
			root,
			locations,
			largestPassing: largestPassing(rows),
		});
		console.log(`wrote ${file}`);
	});
}
