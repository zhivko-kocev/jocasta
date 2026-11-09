/**
 * Jocasta's local collab relay (0536, 0561): every omp pane Jocasta starts hosts its session into it
 * (`collab.autoStart: control`), so each omp publishes itself in omp's local host registry, where the service reads
 * whether it works or asks (`hosts.ts`). Content-blind, 127.0.0.1 only, one fixed port; it stops with Jocasta (0549).
 * The relay contract is omp 18.18.2's (`src/collab/relay-client.ts`, the spike's `relay.ts`): `GET /r/<roomId>?role=
 * host|guest` upgrades to a WebSocket; binary frames are `[4-byte big-endian peer id][sealed payload]`, a host's peer
 * id 0 meaning every guest; a guest's frames reach the host with the guest's id; the host hears
 * `{"t":"peer-joined"|"peer-left","peer":N}`, a guest `{"t":"room-closed"}`; close codes 4001 room closed, 4004 no such
 * room, 4009 a second host. Pings are WebSocket pings, which Bun answers itself.
 */
import { delimiter } from "node:path";
import type { Server, ServerWebSocket } from "bun";
import { UserError } from "../../shared/errors.ts";
import { asObject } from "../../shared/fsx.ts";
import { runProgram } from "../../shared/process.ts";

/** The relay's port on 127.0.0.1 (0561): fixed, so every pane's overlay names it. */
export const RELAY_PORT = 47_499;

/** The relay's URL for omp's `collab.relayUrl`. */
export function relayUrl(port: number): string {
	return `ws://127.0.0.1:${port}`;
}

/**
 * omp's `share.serverUrl` for Jocasta's panes (0561): `/share` uploads to the relay's port, which serves nothing but
 * rooms, so a share never leaves this machine.
 */
export function shareUrl(port: number): string {
	return `http://127.0.0.1:${port}/s`;
}

/** The overlay lines every Jocasta omp pane carries (0536, 0561), as YAML. */
export function relayOverlay(port: number): string[] {
	return [
		"collab:",
		"  autoStart: control",
		`  relayUrl: ${JSON.stringify(relayUrl(port))}`,
		"share:",
		`  serverUrl: ${JSON.stringify(shareUrl(port))}`,
	];
}

/**
 * Whether omp would dial `value` on this machine only: a `ws://` or `wss://` URL to `localhost`, a 127.x address or
 * `::1`. omp reads a value without a scheme as `wss://<value>` (`collab/controller.ts` `#resolveRelayUrl`).
 */
export function isLoopbackRelay(value: string): boolean {
	const text = value.trim();
	if (!text) return false;
	let url: URL;
	try {
		url = new URL(text.includes("://") ? text : `wss://${text}`);
	} catch {
		return false;
	}
	if (url.protocol !== "ws:" && url.protocol !== "wss:") return false;
	const host = url.hostname.toLowerCase();
	return host === "localhost" || host === "[::1]" || /^127(\.\d{1,3}){3}$/.test(host);
}

/** The `--config` files of an omp argv, in order. */
export function configFiles(command: string[]): string[] {
	const files: string[] = [];
	command.forEach((arg, i) => {
		const next = command[i + 1];
		if (arg === "--config" && next !== undefined) files.push(next);
		else if (arg.startsWith("--config=")) files.push(arg.slice("--config=".length));
	});
	return files;
}

/** How long `omp config get` may take before the start is refused (it takes about 0.7 s). */
const RESOLVE_TIMEOUT_MS = 20_000;

/**
 * `collab.relayUrl` as the omp of `command` would resolve it in `cwd` with `env` (0561): omp's own
 * `omp config get collab.relayUrl --json` in that folder, with the pane's `--config` files as `PI_CONFIG_FILES` after
 * any the environment names. omp loads `PI_CONFIG_FILES` and then `--config` into the same overlay list
 * (`config/settings.ts`, `main.ts` `Settings.init`), so the value is the one the pane gets: his config, the project's
 * own `.omp/config.yml` and every overlay, as omp merges them. Asked of omp, not merged here, since a merge of our own
 * would miss whatever omp adds to its layers.
 */
export async function ompRelayUrl(
	omp: string[],
	command: string[],
	cwd: string,
	env: Record<string, string> = {},
): Promise<string> {
	const merged = { ...process.env, ...env };
	const files = [...(merged.PI_CONFIG_FILES?.split(delimiter).filter(Boolean) ?? []), ...configFiles(command)];
	if (omp.length === 0) throw new Error("no omp command");
	const run = await runProgram([...omp, "config", "get", "collab.relayUrl", "--json"], {
		cwd,
		env: { ...merged, PI_CONFIG_FILES: files.join(delimiter) },
		timeoutMs: RESOLVE_TIMEOUT_MS,
	});
	const out = run.stdout;
	if (run.code !== 0) {
		throw new Error(`\`omp config get collab.relayUrl\` failed (exit ${run.code}): ${run.stderr.trim() || out.trim()}`);
	}
	let answer: Record<string, unknown> | undefined;
	try {
		answer = asObject(JSON.parse(out));
	} catch {
		answer = undefined;
	}
	if (!answer) {
		throw new Error(`\`omp config get collab.relayUrl --json\` answered no JSON object: ${out.trim().slice(0, 200)}`);
	}
	return typeof answer.value === "string" ? answer.value : "";
}

/** The refusal of a pane whose relay isn't loopback (0536, 0561). */
export function notLoopback(cwd: string, value: string, files: string[]): UserError {
	return new UserError(
		`omp wasn't started in ${cwd}: its collab.relayUrl resolves to ${value ? `"${value}"` : "nothing"}, not Jocasta's relay on 127.0.0.1, so it would host its session elsewhere (0561). Its --config files: ${files.join(", ") || "none"}.`,
	);
}

/** A running relay. */
export interface Relay {
	port: number;
	/** Closes every room and stops listening; again is a no-op. */
	stop(): void;
}

interface Peer {
	roomId: string;
	role: "host" | "guest";
	peer: number;
}

interface Room {
	host: ServerWebSocket<Peer>;
	guests: Map<number, ServerWebSocket<Peer>>;
	nextPeer: number;
}

const ROOM_PATH = /^\/r\/([A-Za-z0-9_-]{10,64})$/;
/**
 * The relay's bounds (SEC-NET-7): Jocasta's panes host one room each and it has no guests of its own, so these leave
 * room for his own `/join` while a web page can't grow the service's memory. omp keeps a frame's payload under 1 MB
 * plus its envelope (`collab/replication-shrink.ts` `MAX_REPLICATED_PAYLOAD_BYTES`).
 */
const MAX_ROOMS = 64;
const MAX_GUESTS = 4;
const MAX_FRAME_BYTES = 4 * 1024 * 1024;

/** Whether a WebSocket's `Origin` may open a room: none (omp's client sends none) or a loopback page. */
function originAllowed(origin: string | null): boolean {
	if (origin === null) return true;
	if (!URL.canParse(origin)) return false;
	const host = new URL(origin).hostname.toLowerCase();
	return host === "localhost" || host === "[::1]" || /^127(\.\d{1,3}){3}$/.test(host);
}

/**
 * Starts the relay on 127.0.0.1:`port` (`0` picks a free one: tests). A port another program holds fails the start
 * with a UserError naming it: Jocasta doesn't run without its relay.
 */
export function startRelay(port = RELAY_PORT): Relay {
	const rooms = new Map<string, Room>();
	let server: Server<Peer>;
	try {
		server = Bun.serve<Peer>({
			hostname: "127.0.0.1",
			port,
			fetch(request, srv) {
				const url = new URL(request.url);
				const room = ROOM_PATH.exec(url.pathname)?.[1];
				const role = url.searchParams.get("role");
				if (!room || (role !== "host" && role !== "guest")) return new Response("Not found", { status: 404 });
				// A web page elsewhere can't open rooms here (browsers allow cross-origin WebSockets).
				if (!originAllowed(request.headers.get("origin"))) return new Response("Forbidden", { status: 403 });
				const open = rooms.get(room);
				if (role === "host" ? !open && rooms.size >= MAX_ROOMS : (open?.guests.size ?? 0) >= MAX_GUESTS) {
					return new Response("Too many", { status: 503 });
				}
				if (srv.upgrade(request, { data: { roomId: room, role, peer: 0 } })) return undefined;
				return new Response("Upgrade required", { status: 426 });
			},
			websocket: {
				maxPayloadLength: MAX_FRAME_BYTES,
				open(ws) {
					const { roomId, role } = ws.data;
					const room = rooms.get(roomId);
					if (role === "host") {
						if (room) {
							ws.close(4009, "host already connected");
							return;
						}
						rooms.set(roomId, { host: ws, guests: new Map(), nextPeer: 1 });
						return;
					}
					if (!room) {
						ws.close(4004, "no such room");
						return;
					}
					const peer = room.nextPeer++;
					ws.data.peer = peer;
					room.guests.set(peer, ws);
					room.host.send(JSON.stringify({ t: "peer-joined", peer }));
				},
				message(ws, message) {
					const { roomId, role, peer } = ws.data;
					const room = rooms.get(roomId);
					if (!room || typeof message === "string") return;
					const frame = new Uint8Array(message);
					if (frame.byteLength < 4) return;
					const view = new DataView(frame.buffer, frame.byteOffset, 4);
					if (role === "host") {
						if (room.host !== ws) return;
						const target = view.getUint32(0, false);
						if (target === 0) for (const guest of room.guests.values()) guest.send(frame);
						else room.guests.get(target)?.send(frame);
						return;
					}
					view.setUint32(0, peer, false);
					room.host.send(frame);
				},
				close(ws) {
					const { roomId, role, peer } = ws.data;
					const room = rooms.get(roomId);
					if (!room) return;
					if (role === "host" && room.host === ws) {
						rooms.delete(roomId);
						for (const guest of room.guests.values()) {
							guest.send(JSON.stringify({ t: "room-closed" }));
							guest.close(4001, "room closed");
						}
					} else if (role === "guest" && room.guests.delete(peer)) {
						room.host.send(JSON.stringify({ t: "peer-left", peer }));
					}
				},
			},
		});
	} catch (error) {
		throw new UserError(
			`Jocasta's collab relay couldn't listen on 127.0.0.1:${port}: ${(error as Error).message}. Another program holds the port; Jocasta needs it for its omp panes (0561).`,
		);
	}
	let stopped = false;
	return {
		port: server.port as number,
		stop: () => {
			if (stopped) return;
			stopped = true;
			rooms.clear();
			server.stop(true);
		},
	};
}
