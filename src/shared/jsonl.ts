/**
 * JSON lines: omp's session files, the benchmark's rows and logs. omp ≥ 18.10.4 rewrites a session whole through a
 * temp file and a rename (discards, retry marks, shake), so an incremental reader also checks the file's identity; the
 * readers here only keep the byte offset after the last complete line and parse what follows it (CR-03). A line still
 * being written, or torn by a kill, is skipped, never thrown on.
 */
import { appendFileSync, closeSync, existsSync, mkdirSync, openSync, readFileSync, readSync } from "node:fs";
import { dirname } from "node:path";
import { asObject } from "./fsx.ts";

/** Bytes read per call to `readSync`: bounds memory on a first read of a large session. */
const CHUNK = 4 << 20;

/**
 * Hands each complete line of `file` between byte `from` and `to` to `onLine`, in order, and returns the offset just
 * after the last complete line (a line still being written is read again next time).
 */
export function readLines(file: string, from: number, to: number, onLine: (line: string) => void): number {
	if (to <= from) return from;
	const fd = openSync(file, "r");
	let offset = from;
	let carry = Buffer.alloc(0);
	try {
		for (let position = from; position < to; ) {
			const chunk = Buffer.allocUnsafe(Math.min(CHUNK, to - position));
			const n = readSync(fd, chunk, 0, chunk.length, position);
			if (n === 0) break;
			position += n;
			const data = carry.length > 0 ? Buffer.concat([carry, chunk.subarray(0, n)]) : chunk.subarray(0, n);
			let start = 0;
			for (let newline = data.indexOf(0x0a); newline !== -1; newline = data.indexOf(0x0a, start)) {
				onLine(data.toString("utf8", start, newline));
				start = newline + 1;
			}
			offset += start;
			carry = data.subarray(start);
		}
	} finally {
		closeSync(fd);
	}
	return offset;
}

/** A JSONL line as an object; undefined for anything else (a blank line, plain text, or one still being written). */
export function parseLine(line: string): Record<string, unknown> | undefined {
	const trimmed = line.trim();
	if (!trimmed.startsWith("{")) return undefined;
	try {
		return asObject(JSON.parse(trimmed));
	} catch {
		return undefined;
	}
}

/** The JSON object lines of a JSONL text, in order; blank, plain-text and torn lines are skipped. */
export function jsonLines<T = Record<string, unknown>>(text: string): T[] {
	const rows: T[] = [];
	for (const line of text.split("\n")) {
		const row = parseLine(line);
		if (row) rows.push(row as T);
	}
	return rows;
}

/** `jsonLines` of a file; empty when the file doesn't exist. */
export function readJsonlFile<T = Record<string, unknown>>(file: string): T[] {
	return existsSync(file) ? jsonLines<T>(readFileSync(file, "utf8")) : [];
}

/** Appends `row` as one JSON line to `file`, making its folder when missing. */
export function appendJsonl(file: string, row: unknown): void {
	mkdirSync(dirname(file), { recursive: true });
	appendFileSync(file, `${JSON.stringify(row)}\n`);
}
