import { expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { delimiter, join } from "node:path";
import { readHosts } from "../../../src/frontier/server/hosts.ts";
import {
	configFiles,
	isLoopbackRelay,
	ompRelayUrl,
	relayOverlay,
	startRelay,
} from "../../../src/frontier/server/relay.ts";
import { UserError } from "../../../src/shared/errors.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { fakeHost } from "./fake-hosts.ts";
import { until } from "./helpers.ts";

const env = useTempJocasta();

test("a relay is loopback only on localhost, 127.x or ::1 over ws or wss; a bare host is wss (0561)", () => {
	for (const ok of ["ws://127.0.0.1:47499", "wss://localhost", "ws://[::1]:1", "127.0.0.2:9", "WS://LOCALHOST:1"]) {
		expect(isLoopbackRelay(ok)).toBe(true);
	}
	for (const no of [
		"",
		"wss://my.omp.sh",
		"my.omp.sh",
		"http://127.0.0.1:1",
		"ws://127.0.0.1.example.com",
		"ws://10.0.0.1",
	]) {
		expect(isLoopbackRelay(no)).toBe(false);
	}
});

test("the overlay's relay keys name the relay's port and a loopback share server (0536, 0561)", () => {
	expect(Bun.YAML.parse(relayOverlay(47_499).join("\n"))).toEqual({
		collab: { autoStart: "control", relayUrl: "ws://127.0.0.1:47499" },
		share: { serverUrl: "http://127.0.0.1:47499/s" },
	});
	expect(configFiles(["omp", "--config", "a.yml", "-p", "--config=b.yml", "--config"])).toEqual(["a.yml", "b.yml"]);
});

test("collab.relayUrl is asked of omp in the pane's folder with the pane's --config files after the env's own (0561)", async () => {
	const fake = join(env.root, "fake-omp-config.ts");
	writeFileSync(
		fake,
		`console.log(JSON.stringify({ key: "collab.relayUrl", value: [process.argv.slice(2).join(" "), process.env.PI_CONFIG_FILES, process.cwd()].join("|") }, null, 2));`,
	);
	const cwd = join(env.root, "pane");
	mkdirSync(cwd, { recursive: true });
	const value = await ompRelayUrl([process.execPath, fake], ["omp", "--config", "g.yml", "--config", "o.yml"], cwd, {
		PI_CONFIG_FILES: "his.yml",
	});
	expect(value).toBe(
		["config get collab.relayUrl --json", ["his.yml", "g.yml", "o.yml"].join(delimiter), cwd].join("|"),
	);
	const failing = join(env.root, "fake-omp-fails.ts");
	writeFileSync(failing, `console.error("Unknown setting"); process.exit(1);`);
	await expect(ompRelayUrl([process.execPath, failing], [], cwd)).rejects.toThrow("Unknown setting");
});

test("the relay forwards a room's frames between its host and guests by peer id, and says who joins and leaves (0536)", async () => {
	const relay = startRelay(0);
	try {
		const url = (role: string) => `ws://127.0.0.1:${relay.port}/r/room-0123456789?role=${role}`;
		const open = async (role: string) => {
			const ws = new WebSocket(url(role));
			ws.binaryType = "arraybuffer";
			const messages: (string | Uint8Array)[] = [];
			ws.onmessage = (event) =>
				messages.push(typeof event.data === "string" ? event.data : new Uint8Array(event.data as ArrayBuffer));
			const opened = Promise.withResolvers<void>();
			ws.onopen = () => opened.resolve();
			await opened.promise;
			return { ws, messages };
		};
		const host = await open("host");
		const guest = await open("guest");
		await until(() => host.messages.length === 1);
		expect(host.messages[0]).toBe(JSON.stringify({ t: "peer-joined", peer: 1 }));
		// A guest's frame reaches the host carrying the guest's id; the host's broadcast (peer 0) reaches the guest.
		guest.ws.send(new Uint8Array([0, 0, 0, 0, 7, 7]));
		await until(() => host.messages.length === 2);
		expect([...(host.messages[1] as Uint8Array)]).toEqual([0, 0, 0, 1, 7, 7]);
		host.ws.send(new Uint8Array([0, 0, 0, 0, 9]));
		await until(() => guest.messages.length === 1);
		expect([...(guest.messages[0] as Uint8Array)]).toEqual([0, 0, 0, 0, 9]);
		// A second host for the room is refused; a guest of no room too.
		const closed = (ws: WebSocket) => {
			const code = Promise.withResolvers<number>();
			ws.onclose = (event) => code.resolve(event.code);
			return code.promise;
		};
		expect(await closed(new WebSocket(url("host")))).toBe(4009);
		expect(await closed(new WebSocket(`ws://127.0.0.1:${relay.port}/r/no-room-000000?role=guest`))).toBe(4004);
		guest.ws.close();
		await until(() => host.messages.length === 3);
		expect(host.messages[2]).toBe(JSON.stringify({ t: "peer-left", peer: 1 }));
		expect((await fetch(`http://127.0.0.1:${relay.port}/s`)).status).toBe(404);
		host.ws.close();
	} finally {
		relay.stop();
	}
});

test("a relay port another program holds fails the start loudly; a stopped relay frees its port (0561)", async () => {
	const relay = startRelay(0);
	expect(() => startRelay(relay.port)).toThrow(UserError);
	expect(() => startRelay(relay.port)).toThrow(`127.0.0.1:${relay.port}`);
	relay.stop();
	relay.stop();
	await expect(fetch(`http://127.0.0.1:${relay.port}/`)).rejects.toThrow();
	startRelay(relay.port).stop();
});

test("the host registry answers each host of Jocasta's pids through its pipe, never another's; a host that doesn't answer is left out (0547, 0561)", async () => {
	const dir = join(env.root, "collab-hosts");
	const ours = await fakeHost(dir, { pid: 101, cwd: "C:/wt/a", busy: true, inputRequired: false });
	const his = await fakeHost(dir, { pid: 202, cwd: "C:/his", busy: false, inputRequired: false });
	const asking = await fakeHost(dir, { pid: 303, cwd: "C:/wt/b", busy: false, inputRequired: true });
	// A torn metadata file, and a host whose process is gone (its endpoint answers nothing): left out.
	writeFileSync(join(dir, "torn.json"), "{");
	const gone = process.platform === "win32" ? "\\\\.\\pipe\\jocasta-test-gone" : join(dir, "gone.sock");
	writeFileSync(
		join(dir, "gone.json"),
		JSON.stringify({ version: 1, instanceId: "gone0000", pid: 404, endpoint: gone, createdAt: 0, token: "t" }),
	);
	try {
		const hosts = await readHosts(new Set([101, 303, 404]), dir);
		expect(hosts.sort((a, b) => a.pid - b.pid)).toEqual([
			{ pid: 101, cwd: "C:/wt/a", busy: true, inputRequired: false, relayConnected: true },
			{ pid: 303, cwd: "C:/wt/b", busy: false, inputRequired: true, relayConnected: true },
		]);
		expect(his.asked).toBe(0);
		ours.state = { ...ours.state, busy: null };
		expect((await readHosts(new Set([101]), dir))[0]?.busy).toBeNull();
		await ours.close();
		expect(await readHosts(new Set([101]), dir)).toEqual([]);
		expect(await readHosts(new Set([101]), join(env.root, "none"))).toEqual([]);
		expect(await readHosts(new Set(), dir)).toEqual([]);
	} finally {
		await Promise.allSettled([ours.close(), his.close(), asking.close()]);
	}
});
