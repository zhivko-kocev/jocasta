import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
	type DiagnoseDeps,
	diagnoseOverlay,
	RECENT_LOG_LINES,
	recentLog,
	runDiagnose,
} from "../../src/setup/diagnose.ts";
import { UserError } from "../../src/shared/errors.ts";
import { removeTree } from "../../src/shared/fsx.ts";
import { FakeHerdr } from "../frontier/server/fake-herdr.ts";

const ASSETS = resolve(import.meta.dir, "..", "..", "config");
let root = "";
let herdr: FakeHerdr;

beforeEach(() => {
	root = realpathSync.native(mkdtempSync(join(tmpdir(), "jocasta-test-diagnose-")));
	herdr = new FakeHerdr();
});
afterEach(() => removeTree(root));

function logs(files: Record<string, number>): void {
	const dir = join(root, "data", "logs");
	mkdirSync(dir, { recursive: true });
	for (const [day, count] of Object.entries(files)) {
		const lines = Array.from(
			{ length: count },
			(_, i) => `${day}T10:00:${String(i % 60).padStart(2, "0")}Z WARN [service] ${day} line ${i}`,
		);
		writeFileSync(join(dir, `jocasta-${day}.log`), `${lines.join("\n")}\n`);
	}
}

function deps(over: Partial<DiagnoseDeps> = {}): DiagnoseDeps {
	return {
		dataDir: join(root, "data"),
		assetsDir: ASSETS,
		omp: ["omp"],
		versions: "jocasta 0.1.0 (omp 18.12.6, herdr 0.9.5, bun 1.4.3)",
		session: "jocasta-test",
		service: async () => ({ state: "down", file: undefined }),
		connect: async () => herdr,
		ensureBoss: async (h) => {
			const id = await h.ensureWorkspace("Jocasta", join(root, "data", "home"), ["omp", "--continue"]);
			return (await h.panes()).find((p) => p.workspaceId === id) as never;
		},
		setupWaiting: async () => "jocasta setup has 2 changes waiting: run `jocasta setup` to see them.",
		now: new Date("2026-10-23T10:30:00Z"),
		...over,
	};
}

test("recentLog keeps the newest lines across days, oldest first, and names the files", () => {
	logs({ "2026-10-21": 10, "2026-10-22": 300, "2026-10-23": 200 });
	const { text, files } = recentLog(join(root, "data", "logs"));
	const lines = text.trimEnd().split("\n");
	expect(lines).toHaveLength(RECENT_LOG_LINES);
	expect(files).toEqual(["jocasta-2026-10-22.log", "jocasta-2026-10-23.log"]);
	expect(lines[0]).toContain("2026-10-22 line 100");
	expect(lines.at(-1)).toContain("2026-10-23 line 199");
	expect(recentLog(join(root, "none"))).toEqual({ text: "", files: [] });
});

test("jocasta diagnose opens a read-only omp pane in workspace Jocasta with the state and the log attached, saying they go to the model provider (0472, 0484)", async () => {
	logs({ "2026-10-23": 3 });
	await herdr.ensureWorkspace("Jocasta", root, ["omp"]);
	const out: string[] = [];
	const code = await runDiagnose(
		deps({ service: async () => ({ state: "busy", file: { port: 47615, token: "secret", pid: 42 } }) }),
		(l) => void out.push(l),
	);
	expect(code).toBe(0);
	const dir = join(root, "data", "diagnose");
	const [workspaceId, tab] = herdr.of("createTab")[0] as [string, { title: string; cwd: string; command: string[] }];
	expect(workspaceId).toBe(herdr.workspaceList[0]?.id as string);
	expect(tab.title).toBe("diagnose");
	expect(tab.cwd).toBe(dir);
	expect(tab.command).toEqual([
		"omp",
		"--config",
		join(dir, "omp.yml"),
		"--tools",
		"read,grep,glob",
		`@${join(dir, "state.md")}`,
		`@${join(dir, "recent.log")}`,
		"Jocasta's state and recent log, attached, go to the model provider with this message. Diagnose Jocasta with the diagnose skill from them. Change nothing.",
	]);
	expect(herdr.of("focus")).toHaveLength(1);
	expect(out).toEqual([
		"Opened a read-only diagnose pane in workspace Jocasta (herdr session `jocasta-test`): run `jocasta` to see it.",
	]);

	const state = readFileSync(join(dir, "state.md"), "utf8");
	expect(state).toContain("(2026-10-23T10:30:00.000Z)");
	expect(state).toContain("- Versions: jocasta 0.1.0 (omp 18.12.6, herdr 0.9.5, bun 1.4.3)");
	expect(state).toContain("- Service: busy (frontier.json names pid 42, port 47615)");
	expect(state).toContain("- herdr session `jocasta-test`: herdr 0.9.4 answers; 1 workspaces, 1 panes");
	expect(state).toContain("- Setup: jocasta setup has 2 changes waiting");
	expect(state).toContain("(from jocasta-2026-10-23.log)");
	expect(state).toMatch(/^- Jocasta \(w_\d+\)/m);
	// The companion's token never reaches the pane.
	expect(state).not.toContain("secret");
	expect(readFileSync(join(dir, "recent.log"), "utf8")).toContain("2026-10-23 line 2");
	const overlay = Bun.YAML.parse(readFileSync(join(dir, "omp.yml"), "utf8"));
	expect(overlay).toEqual({
		plan: { defaultOnStartup: true },
		skills: { customDirectories: [join(ASSETS, "skills")] },
	});
});

test("without workspace Jocasta diagnose makes it first; a herdr that can't start is a plain error", async () => {
	await runDiagnose(deps(), () => {});
	expect(herdr.of("ensureWorkspace")).toHaveLength(1);
	expect(herdr.workspaceList.map((w) => w.label)).toEqual(["Jocasta"]);
	expect(herdr.of("createTab")).toHaveLength(1);

	const failing = runDiagnose(
		deps({
			connect: async () => {
				throw new Error("herdr isn't installed");
			},
		}),
		() => {},
	);
	await expect(failing).rejects.toBeInstanceOf(UserError);
});

test("the shipped diagnose skill is one omp finds in the overlay's folder: name and description", () => {
	const skill = readFileSync(join(ASSETS, "skills", "diagnose", "SKILL.md"), "utf8");
	const front = skill.match(/^---\n([\s\S]*?)\n---\n/)?.[1] ?? "";
	expect(Bun.YAML.parse(front)).toMatchObject({ name: "diagnose", description: expect.any(String) });
	const mise = readFileSync(join(ASSETS, "skills", "mise", "SKILL.md"), "utf8");
	expect(Bun.YAML.parse(mise.match(/^---\n([\s\S]*?)\n---\n/)?.[1] ?? "")).toMatchObject({ name: "mise" });
	expect(diagnoseOverlay("C:\\x\\skills")).toContain('- "C:\\\\x\\\\skills"');
});
