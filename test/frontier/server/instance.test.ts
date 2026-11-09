// One service at a time (0362, 0549), and a hung one never blocks every later start (0534).
import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { takeInstance } from "../../../src/frontier/server/instance.ts";
import type { ServiceState } from "../../../src/frontier/server/launch.ts";
import { killTree, processIdOf } from "../../../src/frontier/server/proctree.ts";
import { pidAlive } from "../../../src/shared/fsx.ts";
import { useTempJocasta } from "../../shared/fixture.ts";

const env = useTempJocasta();

function lockFile(): string {
	mkdirSync(env.data, { recursive: true });
	return join(env.data, "service.lock");
}

test("a free lock is taken and given back; a dead holder's lock is taken at once", async () => {
	const file = lockFile();
	const self = processIdOf(process.pid);
	const mine = await takeInstance({ lockFile: file, state: async () => "down", kill: async () => {} });
	expect(mine).toBeDefined();
	expect(JSON.parse(readFileSync(file, "utf8"))).toEqual(self);
	mine?.release();
	expect(existsSync(file)).toBe(false);
	// A crashed holder (its pid runs no more, or another program reuses it with another start time).
	writeFileSync(file, JSON.stringify({ pid: process.pid, start: 1 }));
	const again = await takeInstance({ lockFile: file, state: async () => "busy", kill: async () => {} });
	expect(again).toBeDefined();
	again?.release();
});

test("a holder that answers keeps the lock: this start leaves", async () => {
	const file = lockFile();
	writeFileSync(file, JSON.stringify(processIdOf(process.pid)));
	const other = { pid: process.pid, start: processIdOf(process.pid)?.start as number };
	let killed = 0;
	const taken = await takeInstance({
		lockFile: file,
		self: { pid: other.pid + 1, start: 1 },
		state: async () => "up",
		kill: async () => void killed++,
	});
	expect(taken).toBeUndefined();
	expect(killed).toBe(0);
});

test("a live holder that doesn't answer within the bound is hung: its tree is ended and the lock taken over (0534)", async () => {
	const file = lockFile();
	// A hung service: a live process holding the lock whose /health never answers (`busy`).
	const hung = Bun.spawn([process.execPath, "-e", "setInterval(() => {}, 1000)"], {
		stdio: ["ignore", "ignore", "ignore"],
	});
	try {
		let holder = processIdOf(hung.pid);
		for (let i = 0; !holder && i < 50; i++) {
			await Bun.sleep(20);
			holder = processIdOf(hung.pid);
		}
		if (!holder) throw new Error("the stand-in service didn't start");
		writeFileSync(file, JSON.stringify(holder));
		const states: ServiceState[] = [];
		const started = Date.now();
		const mine = await takeInstance({
			lockFile: file,
			state: async () => {
				states.push("busy");
				return "busy";
			},
			kill: killTree,
			hungMs: 300,
			sleep: (ms) => Bun.sleep(Math.min(ms, 50)),
		});
		expect(mine).toBeDefined();
		expect(Date.now() - started).toBeGreaterThanOrEqual(300);
		expect(states.length).toBeGreaterThan(1);
		await hung.exited;
		expect(pidAlive(hung.pid)).toBe(false);
		expect(JSON.parse(readFileSync(file, "utf8"))).toEqual(processIdOf(process.pid));
		mine?.release();
	} finally {
		hung.kill();
	}
});
