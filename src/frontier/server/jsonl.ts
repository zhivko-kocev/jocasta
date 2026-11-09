/**
 * Reading omp's session files incrementally (CR-03): they are append-only JSONL (only line 1's fixed-width title slot
 * is rewritten in place, `session-entries.ts:17-33`), so a reader keeps the byte offset after the last complete line
 * and parses only what was appended since.
 */
import { closeSync, openSync, readSync } from "node:fs";
import { asObject } from "../../shared/fsx.ts";

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

/** A JSONL line as an object; undefined for anything else (a blank line, or one still being written). */
export function parseLine(line: string): Record<string, unknown> | undefined {
	const trimmed = line.trim();
	if (!trimmed.startsWith("{")) return undefined;
	try {
		return asObject(JSON.parse(trimmed));
	} catch {
		return undefined;
	}
}
