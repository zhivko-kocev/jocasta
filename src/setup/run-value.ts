// The logon Run value the companion app wrote (0367) until there was no app (0552, 0557): setup and uninstall remove it.
import { spawnSync } from "node:child_process";

export const RUN_KEY = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/** The released companion's Run value name (0367). */
export const RUN_VALUE = "Jocasta";

/** Whether a Run value's data started the companion's launcher, `"<install dir>\app\bin\launcher.exe"`. */
export function startsCompanion(data: string): boolean {
	return /^"[^"]*\\app\\bin\\launcher\.exe"$/i.test(data.trim());
}

/** The Run value's data now, or undefined when there is none. */
export function readRunValue(name: string): string | undefined {
	const result = spawnSync("reg.exe", ["query", RUN_KEY, "/v", name], { encoding: "utf8", windowsHide: true });
	if (result.status !== 0) return undefined;
	const line = result.stdout.split(/\r?\n/).find((l) => l.trimStart().startsWith(`${name} `));
	return line?.split(/\s+REG_SZ\s+/)[1]?.trim();
}

/** Deletes Run value `name`; true once it is gone. */
export function deleteRunValue(name: string): boolean {
	return spawnSync("reg.exe", ["delete", RUN_KEY, "/v", name, "/f"], { windowsHide: true }).status === 0;
}
