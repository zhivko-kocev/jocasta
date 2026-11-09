import { expect, test } from "bun:test";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
	CALENDAR_ENV,
	deleteCredential,
	readCredential,
	takeCalendarEnv,
	writeCredential,
} from "../../src/shared/credentials.ts";
import { sameToken, writeAtomically } from "../../src/shared/fsx.ts";
import { installDir, selfCommand, shippedConfigDir } from "../../src/shared/install.ts";
import { LOG_DAYS, log, logsDir } from "../../src/shared/log.ts";
import { pidAlive, processStartedAt } from "../../src/shared/proctree.ts";
import { useTempJocasta } from "./fixture.ts";

const env = useTempJocasta();

test("writeAtomically leaves the whole file and no temp file behind", () => {
	const file = join(env.root, "deep", "state.json");
	writeAtomically(file, "one\n");
	writeAtomically(file, "two\n");
	expect(readFileSync(file, "utf8")).toBe("two\n");
	expect(readdirSync(join(env.root, "deep"))).toEqual(["state.json"]);
});

test("pidAlive and sameToken", () => {
	expect(pidAlive(process.pid)).toBe(true);
	expect(pidAlive(0)).toBe(false);
	expect(sameToken("abc", "abc")).toBe(true);
	expect(sameToken("abd", "abc")).toBe(false);
	expect(sameToken(undefined, "abc")).toBe(false);
	// Same length in characters, not in UTF-8 bytes: unequal, never a RangeError (SE-3).
	expect(sameToken("é".repeat(64), "a".repeat(64))).toBe(false);
});

test.skipIf(process.platform !== "win32")(
	"processStartedAt: this process's start time; undefined for none (LC-4)",
	() => {
		const started = processStartedAt(process.pid);
		expect(started).toBeNumber();
		expect(started as number).toBeLessThanOrEqual(Date.now());
		expect(started as number).toBeGreaterThan(Date.now() - 3_600_000);
		expect(processStartedAt(0)).toBeUndefined();
	},
);

test("Jocasta's log: one file per local day in <data>/logs, 7 days kept (0431)", () => {
	const day = Date.parse("2026-10-14T12:00:00");
	mkdirSync(logsDir(), { recursive: true });
	for (const old of ["2026-10-01", "2026-10-07", "2026-10-08"]) {
		writeFileSync(join(logsDir(), `jocasta-${old}.log`), "old\n");
	}
	writeFileSync(join(logsDir(), "other.txt"), "kept\n");
	log("warn", "herdr", "herdr 0.10.0 is running\nsecond line", day);
	const text = readFileSync(join(logsDir(), "jocasta-2026-10-14.log"), "utf8");
	expect(text).toContain("WARN [herdr] herdr 0.10.0 is running\n    second line\n");
	// Today and the 6 days before stay.
	expect(LOG_DAYS).toBe(7);
	expect(readdirSync(logsDir()).sort()).toEqual(["jocasta-2026-10-08.log", "jocasta-2026-10-14.log", "other.txt"]);
});

test("the install dir: jocasta.exe's folder; a source run has none", () => {
	const exe = process.platform === "win32" ? "jocasta.exe" : "jocasta";
	const dir = join(env.root, "Programs", "jocasta");
	mkdirSync(dir, { recursive: true });
	writeFileSync(join(dir, exe), "");
	expect(installDir(join(dir, exe))).toBe(dir);
	expect(shippedConfigDir(join(dir, exe))).toBe(join(dir, "config"));
	expect(installDir(join(env.root, "elsewhere", "bun.exe"))).toBeUndefined();
	// omp and bare `jocasta` start the install's jocasta.exe (0466, 0552).
	expect(selfCommand(join(dir, exe))).toEqual([join(dir, exe)]);
	expect(selfCommand(join(env.root, "elsewhere", "bun.exe"))[1]).toEndWith(join("src", "entry.ts"));
});

test("the calendar's env fallback leaves the environment once taken (SEC-5)", () => {
	process.env[CALENDAR_ENV] = "https://calendar.example/secret.ics";
	expect(takeCalendarEnv()).toBe("https://calendar.example/secret.ics");
	expect(process.env[CALENDAR_ENV]).toBeUndefined();
	expect(takeCalendarEnv()).toBe("https://calendar.example/secret.ics");
});

test.skipIf(process.platform !== "win32")("Credential Manager round trip on a throwaway target (0430)", () => {
	const target = `jocasta-test/platform-${process.pid}`;
	try {
		expect(readCredential(target)).toBeUndefined();
		writeCredential(target, "https://calendar.example/secret.ics");
		expect(readCredential(target)).toBe("https://calendar.example/secret.ics");
	} finally {
		deleteCredential(target);
	}
	expect(readCredential(target)).toBeUndefined();
});
