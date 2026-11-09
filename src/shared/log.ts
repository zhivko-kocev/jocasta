/**
 * Jocasta's log (0431): warnings and errors appended to `<data>/logs/jocasta-YYYY-MM-DD.log` (local day),
 * files older than 7 days removed on the first write of a day. Each line is also written to stderr (a source run).
 * Logging never throws: a full disk must not break the caller.
 */
import { appendFileSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { dayStart, localDay } from "./day.ts";
import { dataDir } from "./paths.ts";

type LogLevel = "info" | "warn" | "error";

/** Days of log files kept, today included. */
export const LOG_DAYS = 7;

const FILE = /^jocasta-(\d{4}-\d{2}-\d{2})\.log$/;

/** `<data>/logs`. */
export function logsDir(): string {
	return join(dataDir(), "logs");
}

/** The folder and day pruned last, so pruning runs once per day and folder. */
let pruned = "";

/** Removes log files older than `LOG_DAYS` days before `today` (`YYYY-MM-DD`). */
function pruneLogs(dir: string, today: string): void {
	const oldest = localDay(dayStart(today, -(LOG_DAYS - 1)));
	for (const name of readdirSync(dir)) {
		const found = FILE.exec(name);
		if (found?.[1] && found[1] < oldest) rmSync(join(dir, name), { force: true });
	}
}

/** Appends one line to today's log file and stderr. */
export function log(level: LogLevel, scope: string, message: string, now = Date.now()): void {
	const line = `${new Date(now).toISOString()} ${level.toUpperCase()} [${scope}] ${message.replaceAll(/\r?\n/g, "\n    ")}\n`;
	process.stderr.write(line);
	try {
		const dir = logsDir();
		mkdirSync(dir, { recursive: true });
		const today = localDay(now);
		appendFileSync(join(dir, `jocasta-${today}.log`), line);
		if (pruned !== `${dir}|${today}`) {
			pruned = `${dir}|${today}`;
			pruneLogs(dir, today);
		}
	} catch {
		// The log is best effort; stderr has the line.
	}
}
