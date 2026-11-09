import { describe, expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, sep } from "node:path";
import { findListedPanes } from "../../bench/h2-toast.ts";
import { installedJocasta } from "../../bench/h9-footprint.ts";
import { useTempDirs } from "../shared/fixture.ts";

const tempDir = useTempDirs("jocasta-bench-build-");

describe("H2 reads its threads as `jocasta thread list` does (0576)", () => {
	test("a started thread gets its worktree's pane only once the list shows its branch, and keeps it", () => {
		// herdr reports a pane's folder in the host's own form.
		const wt = join(tmpdir(), "src", "acme.wt");
		const panes = [
			{ paneId: "boss", cwd: join(tmpdir(), "data", "home") },
			{ paneId: "p1", cwd: join(wt, "h2-trial-1") },
			{ paneId: "p2", cwd: `${join(wt, "h2-trial-2")}${sep}` },
		];
		const ours: { branch: string; paneId?: string }[] = [
			{ branch: "feature/h2-trial-1" },
			{ branch: "feature/h2-trial-2" },
		];
		findListedPanes(ours, [{ branch: "feature/h2-trial-1" }, { branch: "feature/other" }], panes);
		expect(ours).toEqual([{ branch: "feature/h2-trial-1", paneId: "p1" }, { branch: "feature/h2-trial-2" }]);
		findListedPanes(
			ours,
			[{ branch: "feature/h2-trial-2" }],
			[{ paneId: "moved", cwd: join(wt, "h2-trial-1") }, ...panes],
		);
		expect(ours).toEqual([
			{ branch: "feature/h2-trial-1", paneId: "p1" },
			{ branch: "feature/h2-trial-2", paneId: "p2" },
		]);
	});
});

describe("H9 names the build it installed (0576 BEN-22)", () => {
	test("the jocasta on PATH, else the install folder's jocasta.exe, else none", () => {
		const onPath = tempDir();
		const installDir = tempDir();
		const local = tempDir();
		const exe = process.platform === "win32" ? "jocasta.exe" : "jocasta";
		// PATH predates the install: JOCASTA_INSTALL_DIR's exe, else %LOCALAPPDATA%\Programs\jocasta's.
		writeFileSync(join(installDir, "jocasta.exe"), "");
		expect(installedJocasta({ PATH: onPath, JOCASTA_INSTALL_DIR: installDir, LOCALAPPDATA: local })).toBe(
			join(installDir, "jocasta.exe"),
		);
		expect(installedJocasta({ PATH: onPath, LOCALAPPDATA: local })).toBeUndefined();
		mkdirSync(join(local, "Programs", "jocasta"), { recursive: true });
		writeFileSync(join(local, "Programs", "jocasta", "jocasta.exe"), "");
		expect(installedJocasta({ PATH: onPath, LOCALAPPDATA: local })).toBe(
			join(local, "Programs", "jocasta", "jocasta.exe"),
		);
		// On PATH, that one wins.
		writeFileSync(join(onPath, exe), "", { mode: 0o755 });
		expect(installedJocasta({ PATH: onPath, JOCASTA_INSTALL_DIR: installDir, LOCALAPPDATA: local })).toBe(
			join(onPath, exe),
		);
	});
});
