import { expect, test } from "bun:test";
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
	countComments,
	findHunk,
	hunkPackage,
	listComments,
	proofSaid,
	reviewPrompt,
	reviewProof,
} from "../../../src/frontier/server/hunk.ts";
import { makeRepo, sh, useTempJocasta } from "../../shared/fixture.ts";

const env = useTempJocasta();

test("hunk's npm platform packages (0394)", () => {
	expect(hunkPackage("win32", "x64")).toBe("hunkdiff-windows-x64");
	expect(hunkPackage("linux", "x64")).toBe("hunkdiff-linux-x64");
	expect(hunkPackage("darwin", "arm64")).toBe("hunkdiff-darwin-arm64");
	expect(hunkPackage("linux", "arm64")).toBeUndefined();
});

test("a source run finds hunk in node_modules/hunkdiff-<platform> through the package's bin", () => {
	const found = findHunk();
	if (!hunkPackage()) return expect(found).toBeUndefined();
	expect(found).toMatch(/node_modules[\\/]hunkdiff-[a-z0-9-]+[\\/]bin[\\/]hunk(\.exe)?$/);
	expect(existsSync(found as string)).toBe(true);
});

test("the review prompt spells out hunk's commands with a quoted, forward-slashed binary", () => {
	const prompt = reviewPrompt(["C:\\Program Files\\jocasta\\hunk\\hunk.exe"], "main...feature/x");
	expect(prompt).toContain("git diff main...feature/x");
	expect(prompt).toContain('`"C:/Program Files/jocasta/hunk/hunk.exe" session comment apply --repo . --stdin`');
	expect(reviewPrompt(["/opt/hunk"], "main...b")).toContain("`/opt/hunk session review --repo . --json`");
});

test("the reviewer gets the plan, the rules and the lessons, and only material findings with priorities (0453, 0459, 0460, 0546)", () => {
	const gates = "checks: FAIL (tree 3f9a1c)\n  FAIL  test  mise run test";
	const prompt = reviewPrompt(["/opt/hunk"], "main...b", {
		plan: "C:\\plans\\p.md",
		rules: ["C:\\wt\\AGENTS.md", "C:\\repo\\AGENTS.local.md"],
		gates,
		checked: true,
	});
	expect(prompt).toContain("the approved plan (C:/plans/p.md)");
	expect(prompt).toContain("the project's rules (C:/wt/AGENTS.md, C:/repo/AGENTS.local.md)");
	expect(prompt).toContain("skill://lessons");
	expect(prompt).toContain("report only material findings; prefer one strong finding over several weak ones");
	// Jocasta ran the checks: the failing gates go in, the turn runs no tests of its own and ends on no TESTS line.
	expect(prompt).toContain(`they failed; give the reviewer these gates too:\n\n${gates}\n\n`);
	expect(prompt).not.toContain("as one bash command");
	expect(prompt).not.toContain("TESTS:");
	expect(prompt).toContain('"summary":"[P<priority>] <one sentence>"');
	// Checks that passed say nothing.
	const passed = reviewPrompt(["/opt/hunk"], "main...b", { rules: [], gates: "", checked: true });
	expect(passed).toStartWith("Check this task's work. When the project has a `verify` skill");
	// Without a mise check, the project's own way and the TESTS line; without a plan or rules, only the lessons.
	const plain = reviewPrompt(["/opt/hunk"], "main...b");
	expect(plain).toContain("the way its AGENTS.md or package scripts say");
	expect(plain).toContain("read skill://verify and run it too");
	expect(plain).toContain("`TESTS: pass — <the test command you ran>`");
	expect(plain).not.toContain("approved plan");
	expect(plain).toContain("give it its lessons in skill://lessons");
});

test("notes are counted from `hunk session comment list --json`, only those made since the review began (CR-22)", async () => {
	expect(countComments('{"comments":[{"commentId":"a"},{"commentId":"b"}]}')).toEqual({ notes: 2, blocking: 0 });
	expect(countComments('{"comments":[]}')).toEqual({ notes: 0, blocking: 0 });
	expect(() => countComments("{}")).toThrow("no comments array");
	const timed = JSON.stringify({
		comments: [
			{ commentId: "old", createdAt: "2026-10-14T09:00:00.000Z" },
			{ commentId: "new", createdAt: "2026-10-14T10:00:00.000Z" },
		],
	});
	expect(countComments(timed, Date.parse("2026-10-14T09:30:00.000Z"))).toEqual({ notes: 1, blocking: 0 });
	const fake = [process.execPath, `${import.meta.dir}/fake-hunk.ts`];
	expect((await listComments(fake, import.meta.dir)).notes).toBe(2);
	await expect(listComments([process.execPath, `${import.meta.dir}/no-such-hunk.ts`], import.meta.dir)).rejects.toThrow(
		"hunk session comment list failed",
	);
});

test("notes the reviewer marked P0 or P1 are blocking (0453)", () => {
	const json = JSON.stringify({
		comments: [
			{ summary: "[P0] Data loss on retry" },
			{ summary: "[P1] Race under load" },
			{ summary: "[P2] Edge case" },
			{ summary: "P3 nit" },
			{ summary: "No priority" },
		],
	});
	expect(countComments(json)).toEqual({ notes: 5, blocking: 2 });
});

test("the review's proof: diff size, changed test files and added skip/only markers, from one diff of the test files (0453, SRV-A-9)", async () => {
	const repo = makeRepo(join(env.root, "proof"));
	sh(repo, "checkout", "-q", "-b", "feature/x");
	writeFileSync(join(repo, "app.ts"), "export const a = 1;\nexport const b = 2;\n");
	writeFileSync(join(repo, "app.test.ts"), 'test.skip("a", () => {});\n');
	writeFileSync(join(repo, "other.test.ts"), 'test("b", () => {});\n');
	sh(repo, "add", ".");
	sh(repo, "commit", "-q", "-m", "work");
	const proof = await reviewProof(repo, "main...feature/x");
	expect(proof).toEqual({
		added: 4,
		deleted: 0,
		files: 3,
		testFiles: ["app.test.ts", "other.test.ts"],
		skipped: ["app.test.ts"],
	});
	expect(proofSaid({ added: 3, deleted: 0, files: 2, testFiles: ["app.test.ts"], skipped: ["app.test.ts"] })).toBe(
		"+3/−0 in 2 files, test files changed: app.test.ts, adds skip/only in app.test.ts",
	);
	expect(proofSaid({ added: 1, deleted: 1, files: 1, testFiles: [], skipped: [] })).toBe(
		"+1/−1 in 1 file, no test files changed",
	);
});
