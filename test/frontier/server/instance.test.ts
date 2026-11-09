// One service at a time (0362, 0549), and a hung one never blocks every later start (0534) nor ends herdr's (0582).
import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { takeInstance } from "../../../src/frontier/server/instance.ts";
import type { ServiceState } from "../../../src/frontier/server/launch.ts";
import { endProcesses, pidAlive, processIdOf } from "../../../src/shared/proctree.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { killAll, settles } from "./sleepers.ts";

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
	// The herdr server it starts is noted beside it (0582).
	mine?.noteHerdr({ pid: 4242, start: 7 });
	expect(JSON.parse(readFileSync(file, "utf8"))).toEqual({ ...self, herdr: { pid: 4242, start: 7 } });
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
		windowsHide: true,
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
			kill: (holder, spare) => endProcesses([holder], spare),
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

test("a hung service's takeover spares the herdr server it started and what runs under it (0582)", async () => {
	const file = lockFile();
	// A hung service that started a "herdr server" (its child, detached as `connectHerdr` starts it), which runs a "pane"
	// of its own; the two write their pids beside the lock. Each stand-in starts hidden with no inherited stdio: the
	// detached one has no console, and a console program it started otherwise would get a console window of its own,
	// which Windows hands to Windows Terminal. A hidden one gets a windowless console host as a child instead, so the
	// stand-ins are named by their pid files, not by their place in the tree.
	const pidFile = (name: string) => join(env.data, `${name}.pid`);
	const announce = (name: string) =>
		`require("node:fs").writeFileSync(${JSON.stringify(pidFile(name))}, String(process.pid));`;
	const idle = "setInterval(() => {}, 1000)";
	const pane = `${announce("pane")} ${idle}`;
	const herdrServer = `${announce("herdr")} Bun.spawn([process.execPath, "-e", ${JSON.stringify(pane)}], { stdio: ["ignore", "ignore", "ignore"], windowsHide: true }); ${idle}`;
	const service = `require("node:child_process").spawn(process.execPath, ["-e", ${JSON.stringify(herdrServer)}], { detached: true, stdio: "ignore", windowsHide: true }).unref(); ${idle}`;
	const hung = Bun.spawn([process.execPath, "-e", service], {
		stdio: ["ignore", "ignore", "ignore"],
		windowsHide: true,
	});
	const spared = () =>
		["herdr", "pane"]
			.map((name) => (existsSync(pidFile(name)) ? Number(readFileSync(pidFile(name), "utf8")) : 0))
			.filter((pid) => pid > 0);
	try {
		expect(await settles(() => spared().length === 2)).toBe(true);
		const holder = processIdOf(hung.pid);
		const herdr = processIdOf(spared()[0] as number);
		if (!holder || !herdr) throw new Error("the stand-in service didn't start its herdr");
		writeFileSync(file, JSON.stringify({ ...holder, herdr }));
		const mine = await takeInstance({
			lockFile: file,
			state: async () => "busy",
			kill: (h, spare) => endProcesses([h], spare),
			hungMs: 0,
			sleep: async () => {},
		});
		expect(mine).toBeDefined();
		await hung.exited;
		for (const pid of spared()) expect(pidAlive(pid)).toBe(true);
		mine?.release();
	} finally {
		hung.kill();
		killAll(spared());
	}
}, 30_000);
