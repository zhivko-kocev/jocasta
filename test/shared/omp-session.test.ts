import { expect, test } from "bun:test";
import { appendFileSync, mkdirSync, renameSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { removeTree } from "../../src/shared/fsx.ts";
import {
	readHeader,
	rewritten,
	SessionTail,
	SessionWalker,
	type TailVisitor,
	transcriptsOf,
} from "../../src/shared/omp-session.ts";
import { useTempDirs } from "./fixture.ts";

const tempDir = useTempDirs("omp-session-");

const title = JSON.stringify({ type: "title", v: 1, title: "  demo  ", pad: "" });
const header = (extra: Record<string, unknown> = {}) =>
	JSON.stringify({
		type: "session",
		version: 3,
		id: "s1",
		timestamp: "2026-11-09T10:00:00.000Z",
		cwd: "C:\\work",
		...extra,
	});

function write(file: string, lines: string[]): string {
	mkdirSync(join(file, ".."), { recursive: true });
	writeFileSync(file, `${lines.join("\n")}\n`);
	return file;
}

/** A visitor that records the ids it was handed and how often it was reset. */
function recorder(): TailVisitor & { ids: string[]; resets: number } {
	return {
		ids: [],
		resets: 0,
		entry(entry) {
			this.ids.push(String(entry.id));
		},
		reset() {
			this.ids = [];
			this.resets++;
		},
	};
}

test("readHeader finds the session line after the title slot", () => {
	const dir = tempDir();
	const file = write(join(dir, "a.jsonl"), [title, header({ parentSession: "s0" }), JSON.stringify({ id: "e1" })]);
	expect(readHeader(file)).toEqual({
		file,
		cwd: "C:\\work",
		title: "demo",
		id: "s1",
		parentSession: "s0",
		started: Date.parse("2026-11-09T10:00:00.000Z"),
	});
	expect(readHeader(write(join(dir, "b.jsonl"), [title]))).toBeUndefined();
	expect(readHeader(write(join(dir, "c.jsonl"), [header({ cwd: 1 })]))).toBeUndefined();
});

test("transcriptsOf lists a session's nested transcripts, sorted, gc-compress scratch left out", () => {
	const dir = tempDir();
	const session = write(join(dir, "s.jsonl"), [header()]);
	const artifacts = join(dir, "s");
	write(join(artifacts, "1-Task.jsonl"), [header()]);
	write(join(artifacts, "0-Explore.jsonl"), [header()]);
	write(join(artifacts, "__advisor.jsonl"), [header()]);
	write(join(artifacts, "0-Explore", "0-Deep.jsonl"), [header()]);
	write(join(artifacts, "0-Explore.omp-gc-compress-123.jsonl"), [header()]);
	write(join(artifacts, ".omp-gc-compress-9", "copy.jsonl"), [header()]);
	writeFileSync(join(artifacts, "notes.md"), "x");
	expect(transcriptsOf(session)).toEqual(
		[
			join(artifacts, "0-Explore", "0-Deep.jsonl"),
			join(artifacts, "0-Explore.jsonl"),
			join(artifacts, "1-Task.jsonl"),
			join(artifacts, "__advisor.jsonl"),
		].sort(),
	);
	expect(transcriptsOf(join(dir, "none.jsonl"))).toEqual([]);
});

test("a SessionTail reads appended lines once, waits for a torn last line and reads a rewrite from its start", () => {
	const dir = tempDir();
	const file = write(join(dir, "s.jsonl"), [header(), JSON.stringify({ id: "e1" })]);
	const seen = recorder();
	const tail = new SessionTail(file, seen);
	expect(tail.advance()).toBe(true);
	expect(seen.ids).toEqual(["s1", "e1"]);
	// Unchanged: nothing is read again.
	expect(tail.advance()).toBe(false);
	// A line still being written waits until it is whole.
	appendFileSync(file, `${JSON.stringify({ id: "e2" })}\n{"id":"e3`);
	tail.advance();
	expect(seen.ids).toEqual(["s1", "e1", "e2"]);
	appendFileSync(file, `"}\n`);
	tail.advance();
	expect(seen.ids).toEqual(["s1", "e1", "e2", "e3"]);
	expect(seen.resets).toBe(0);
	// omp's rewrite through a temp file and a rename, no shorter than what was read: read again from the start.
	const temp = write(join(dir, "s.jsonl.tmp"), [
		header(),
		JSON.stringify({ id: "e1" }),
		JSON.stringify({ id: "r1" }),
		"x".repeat(60),
	]);
	renameSync(temp, file);
	expect(tail.advance()).toBe(true);
	expect(seen.resets).toBe(1);
	expect(seen.ids).toEqual(["s1", "e1", "r1"]);
	// Rewritten in place, shorter than the offset: read again too.
	writeFileSync(file, `${JSON.stringify({ id: "short" })}\n`);
	tail.advance();
	expect(seen.resets).toBe(2);
	expect(seen.ids).toEqual(["short"]);
	// A missing file leaves the tail as it was.
	removeTree(file);
	expect(tail.advance()).toBe(false);
	expect(seen.ids).toEqual(["short"]);
});

test("a SessionTail read in steps reads a line longer than a step whole", async () => {
	const dir = tempDir();
	const long = JSON.stringify({ id: "long", text: "y".repeat(5000) });
	const file = write(join(dir, "s.jsonl"), [header(), long, JSON.stringify({ id: "e2" })]);
	const seen = recorder();
	const tail = new SessionTail(file, seen);
	await tail.advanceInSteps(64, () => false);
	expect(seen.ids).toEqual(["s1", "long", "e2"]);
	expect(tail.offset).toBe(statSync(file).size);
	expect(tail.advance()).toBe(false);
});

test("rewritten: another file under the name, or one shorter than the offset; a mark without identity can't say", () => {
	const dir = tempDir();
	const stat = statSync(write(join(dir, "s.jsonl"), [header()]));
	expect(rewritten(stat, { offset: stat.size, ino: stat.ino, birthtimeMs: stat.birthtimeMs })).toBe(false);
	expect(rewritten(stat, { offset: stat.size + 1, ino: stat.ino, birthtimeMs: stat.birthtimeMs })).toBe(true);
	expect(rewritten(stat, { offset: 0, ino: stat.ino, birthtimeMs: stat.birthtimeMs + 1 })).toBe(true);
	expect(rewritten(stat, { offset: stat.size })).toBe(false);
});

test("a SessionWalker lists the session files of each cwd dir and lists a dir again only when its mtime changed", () => {
	const sessions = tempDir();
	const cwdDir = join(sessions, "--C--work--");
	const first = write(join(cwdDir, "a.jsonl"), [header()]);
	write(join(cwdDir, "a", "0-Task.jsonl"), [header()]);
	writeFileSync(join(sessions, "stray.jsonl"), "");
	const old = new Date(Date.now() - 60_000);
	utimesSync(cwdDir, old, old);
	const walker = new SessionWalker(sessions);
	expect(walker.files()).toEqual([first]);
	// A file added with the dir's mtime set back is not seen: the dir is answered from the last listing.
	const second = write(join(cwdDir, "b.jsonl"), [header()]);
	utimesSync(cwdDir, old, old);
	expect(walker.files()).toEqual([first]);
	// Its mtime changed: listed again.
	const newer = new Date(Date.now() - 30_000);
	utimesSync(cwdDir, newer, newer);
	expect(walker.files().sort()).toEqual([first, second].sort());
	// A fresh dir is listed every walk while its mtime is within the clock's tick of the listing.
	const other = join(sessions, "--C--other--");
	const third = write(join(other, "c.jsonl"), [header()]);
	expect(walker.files()).toContain(third);
	const fourth = write(join(other, "d.jsonl"), [header()]);
	expect(walker.files()).toContain(fourth);
	removeTree(other);
	expect(walker.files().sort()).toEqual([first, second].sort());
	expect(new SessionWalker(join(sessions, "missing")).files()).toEqual([]);
});
