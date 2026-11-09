import { expect, test } from "bun:test";
import { dirname } from "node:path";
import { onPath, runProgram, runProgramSync } from "../../src/shared/process.ts";
import { pidAlive } from "../../src/shared/proctree.ts";
import { settles } from "../frontier/server/sleepers.ts";

const bun = process.execPath;

test("onPath finds a program whatever the PATH variable's case", () => {
	const name = process.platform === "win32" ? "bun.exe" : "bun";
	const folder = dirname(bun);
	expect(onPath(name, { Path: folder })).not.toBeNull();
	expect(onPath(name, { PATH: folder })).not.toBeNull();
	expect(onPath(name, {})).toBeNull();
});

test("runProgram: exit code, both streams, stdin when given, output past the cap dropped", async () => {
	const run = await runProgram(
		[bun, "-e", "process.stdout.write(await Bun.stdin.text()); console.error('err'); process.exit(3)"],
		{ stdin: "hello" },
	);
	expect(run).toEqual({ code: 3, signal: null, stdout: "hello", stderr: "err\n", timedOut: false });
	const capped = await runProgram([bun, "-e", "process.stdout.write('x'.repeat(100000))"], { maxBytes: 10 });
	expect(capped.stdout).toBe("x".repeat(10));
	await expect(runProgram(["jocasta-no-such-program"])).rejects.toThrow();
});

test("runProgram ends the program's whole tree at its limit, and says so", async () => {
	const child = `const c = Bun.spawn([process.execPath, "-e", "setInterval(() => {}, 1000)"], { stdio: ["ignore", "ignore", "ignore"], windowsHide: true }); console.log(c.pid); setInterval(() => {}, 1000);`;
	const run = await runProgram([bun, "-e", child], { timeoutMs: 1_500 });
	expect(run.timedOut).toBe(true);
	expect(run.code).toBeNull();
	const grandchild = Number(run.stdout.trim());
	expect(grandchild).toBeGreaterThan(0);
	expect(await settles(() => !pidAlive(grandchild))).toBe(true);
}, 30_000);

test("runProgram rejects at once with the reason when its signal aborts", async () => {
	const controller = new AbortController();
	const run = runProgram([bun, "-e", "setInterval(() => {}, 1000)"], { signal: controller.signal });
	controller.abort(new Error("stopping"));
	await expect(run).rejects.toThrow("stopping");
}, 30_000);

test("runProgramSync: the same result, and a time limit that reads as code null", () => {
	expect(runProgramSync([bun, "-e", "console.log('ok')"])).toEqual({
		code: 0,
		signal: null,
		stdout: "ok\n",
		stderr: "",
		timedOut: false,
	});
	const slow = runProgramSync([bun, "-e", "setInterval(() => {}, 1000)"], { timeoutMs: 300 });
	expect(slow.timedOut).toBe(true);
	expect(slow.code).toBeNull();
});
