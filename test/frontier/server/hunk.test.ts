import { expect, test } from "bun:test";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { ServiceContext } from "../../../src/frontier/server/context.ts";
import {
	countComments,
	findHunk,
	hunkPackage,
	listComments,
	proofSaid,
	type ReviewProof,
	reviewPrompt,
	reviewProof,
} from "../../../src/frontier/server/hunk.ts";
import { ReviewFlow } from "../../../src/frontier/server/review-flow.ts";
import type { Thread } from "../../../src/frontier/server/thread.ts";
import { shippedConfigDir } from "../../../src/shared/install.ts";
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
		removedTests: [],
		fewerAssertions: [],
	});
	expect(proofSaid({ added: 3, deleted: 0, files: 2, testFiles: ["app.test.ts"], skipped: ["app.test.ts"] })).toBe(
		"+3/−0 in 2 files, test files changed: app.test.ts, adds skip/only in app.test.ts",
	);
	expect(proofSaid({ added: 1, deleted: 1, files: 1, testFiles: [], skipped: [] })).toBe(
		"+1/−1 in 1 file, no test files changed",
	);
});

/** A repo whose trunk holds `base` and whose `feature/x` then applies `change` (a path → text, or null to delete): its proof. */
async function proofOf(name: string, base: Record<string, string>, change: Record<string, string | null>) {
	const repo = makeRepo(join(env.root, name));
	for (const [file, text] of Object.entries(base)) {
		mkdirSync(dirname(join(repo, file)), { recursive: true });
		writeFileSync(join(repo, file), text);
	}
	sh(repo, "add", ".");
	sh(repo, "commit", "-q", "-m", "base");
	sh(repo, "checkout", "-q", "-b", "feature/x");
	for (const [file, text] of Object.entries(change)) {
		if (text === null) rmSync(join(repo, file));
		else {
			mkdirSync(dirname(join(repo, file)), { recursive: true });
			writeFileSync(join(repo, file), text);
		}
	}
	sh(repo, "add", "-A");
	sh(repo, "commit", "-q", "-m", "work");
	return (await reviewProof(repo, "main...feature/x")) as ReviewProof;
}

const TWO_TESTS =
	'import { expect, test } from "bun:test";\ntest("a", () => {\n  expect(1).toBe(1);\n});\ntest("b", () => {\n  expect(2).toBe(2);\n});\n';

test("the proof finds a removed test, a deleted test file, a commented-out test (0610)", async () => {
	const one = TWO_TESTS.split('test("b"')[0] as string;
	const removed = await proofOf("removed", { "a.test.ts": TWO_TESTS }, { "a.test.ts": one });
	expect(removed.removedTests).toEqual(["a.test.ts"]);
	expect(removed.fewerAssertions).toEqual(["a.test.ts"]);
	expect(removed.skipped).toEqual([]);
	const deleted = await proofOf(
		"deleted",
		{ "test/a.test.ts": TWO_TESTS, "src.ts": "x\n" },
		{ "test/a.test.ts": null },
	);
	expect(deleted.removedTests).toEqual(["test/a.test.ts"]);
	const commented = await proofOf(
		"commented",
		{ "a.test.ts": TWO_TESTS },
		{ "a.test.ts": TWO_TESTS.replace('test("b", () => {', '// test("b", () => {') },
	);
	expect(commented.removedTests).toEqual(["a.test.ts"]);
});

test("the proof counts a moved or renamed test file, and a test renamed in place, as no removed tests (0610)", async () => {
	const moved = await proofOf("moved", { "a.test.ts": TWO_TESTS }, { "a.test.ts": null, "test/b.test.ts": TWO_TESTS });
	expect(moved.removedTests).toEqual([]);
	expect(moved.fewerAssertions).toEqual([]);
	const renamed = await proofOf(
		"renamed",
		{ "a.test.ts": TWO_TESTS },
		{ "a.test.ts": TWO_TESTS.replace('test("b"', 'test("b, renamed"') },
	);
	expect(renamed.removedTests).toEqual([]);
	expect(renamed.fewerAssertions).toEqual([]);
});

test("the proof finds fewer assertions in a changed test file, and none in a clean change (0610)", async () => {
	const weaker = await proofOf(
		"weaker",
		{ "a.test.ts": TWO_TESTS },
		{ "a.test.ts": TWO_TESTS.replace("  expect(2).toBe(2);\n", "") },
	);
	expect(weaker.fewerAssertions).toEqual(["a.test.ts"]);
	expect(weaker.removedTests).toEqual([]);
	const skip = await proofOf(
		"skip",
		{ "a.test.ts": TWO_TESTS },
		{ "a.test.ts": TWO_TESTS.replace('test("b"', 'test.skip("b"') },
	);
	expect(skip.skipped).toEqual(["a.test.ts"]);
	const clean = await proofOf(
		"clean",
		{ "a.test.ts": TWO_TESTS, "app.ts": "export const a = 1;\n" },
		{ "a.test.ts": `${TWO_TESTS}test("c", () => {\n  expect(3).toBe(3);\n});\n`, "app.ts": "export const a = 2;\n" },
	);
	expect(clean.removedTests).toEqual([]);
	expect(clean.fewerAssertions).toEqual([]);
	expect(clean.skipped).toEqual([]);
});

test("the proof reads Python, Go, Rust and Ruby tests (0610)", async () => {
	const base = {
		"test_a.py": "def test_a():\n    assert 1 == 1\n\n\ndef test_b():\n    assert 2 == 2\n",
		"a_test.go": 'func TestA(t *testing.T) {\n\tt.Errorf("x")\n}\n\nfunc TestB(t *testing.T) {\n\tt.Errorf("y")\n}\n',
		"tests/a.rs":
			"#[test]\nfn a() {\n    assert_eq!(1, 1);\n}\n\n#[tokio::test]\nasync fn b() {\n    assert!(true);\n}\n",
		"spec/a_spec.rb": 'it "a" do\n  expect(1).to eq(1)\nend\n\nit "b" do\n  expect(2).to eq(2)\nend\n',
	};
	const proof = await proofOf("languages", base, {
		"test_a.py": "def test_a():\n    assert 1 == 1\n",
		"a_test.go": 'func TestA(t *testing.T) {\n\tt.Errorf("x")\n}\n',
		"tests/a.rs": "#[test]\nfn a() {\n    assert_eq!(1, 1);\n}\n",
		"spec/a_spec.rb": 'it "a" do\n  expect(1).to eq(1)\nend\n',
	});
	expect(proof.removedTests).toEqual(["a_test.go", "spec/a_spec.rb", "test_a.py", "tests/a.rs"]);
	expect(proof.fewerAssertions).toEqual(["a_test.go", "spec/a_spec.rb", "test_a.py", "tests/a.rs"]);
});

test("a stored proof from before 0610 says what it knew; the new facts are said when present (0610)", () => {
	const old: ReviewProof = { added: 1, deleted: 0, files: 1, testFiles: ["a.test.ts"], skipped: [] };
	expect(proofSaid(old)).toBe("+1/−0 in 1 file, test files changed: a.test.ts");
	expect(proofSaid({ ...old, removedTests: ["a.test.ts"], fewerAssertions: ["a.test.ts", "b.test.ts"] })).toBe(
		"+1/−0 in 1 file, test files changed: a.test.ts, removes tests in a.test.ts, fewer assertions in a.test.ts, b.test.ts",
	);
});

test("the review prompt gives the reviewer the shipped design rubric and a cap of three non-blocking design findings (0611)", () => {
	const prompt = reviewPrompt(["/opt/hunk"], "main...b");
	expect(prompt).toContain(`${shippedConfigDir().replace(/\\/g, "/")}/design-rubric.md`);
	expect(prompt).toContain("at most three design findings");
	expect(prompt).toContain("the code this change could delete");
	expect(prompt).toContain("[P3]");
	expect(prompt).toContain("never block");
	// A design note as the prompt asks for it is not blocking.
	expect(
		countComments(JSON.stringify({ comments: [{ summary: "[P3] Design: a pass-through method" }] })).blocking,
	).toBe(0);
	expect(existsSync(join(shippedConfigDir(), "design-rubric.md"))).toBe(true);
});

test("a review fails on each way a proof weakens the tests (0610)", () => {
	const flow = new ReviewFlow({} as ServiceContext);
	const proof: ReviewProof = { added: 1, deleted: 1, files: 1, testFiles: ["a.test.ts"], skipped: [] };
	const thread = (changes: Partial<ReviewProof> | undefined) =>
		({
			task: { doneWhen: [] },
			review: {
				result: {
					notes: 0,
					blocking: 0,
					tests: "pass",
					checks: [],
					...(changes ? { proof: { ...proof, ...changes } } : {}),
				},
			},
		}) as unknown as Thread;
	expect(flow.failed(thread({}))).toBeUndefined();
	expect(flow.failed(thread(undefined))).toBeUndefined();
	// A proof stored before 0610 has neither new field.
	expect(flow.failed(thread({ removedTests: undefined }))).toBeUndefined();
	expect(flow.failed(thread({ removedTests: ["a.test.ts"] }))).toBe("removed tests in a.test.ts");
	expect(flow.failed(thread({ fewerAssertions: ["a.test.ts"] }))).toBe("fewer assertions in a.test.ts");
	expect(flow.failed(thread({ skipped: ["a.test.ts"] }))).toBe("added skip/only in a.test.ts");
});
