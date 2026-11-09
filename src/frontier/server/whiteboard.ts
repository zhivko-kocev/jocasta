/**
 * whiteboard's runtime in Jocasta (0393, 0411, 0417, 0419, 0552): its Desktop server (`dist/server/desktop-host.js`,
 * the server whiteboard's own Desktop runs, under plain Node) runs as the service's child with telemetry off and its
 * state in `<data>/whiteboard`; the service's canvas page (`src/frontier/canvas/`, opened in his browser) mounts its
 * canvas (`@dev.fast/review-canvas`) and talks to that server directly on loopback with the server's token. The
 * Desktop server serves no page of its own, only its API. The Desktop server rather than `whiteboard server start`,
 * because only it serves the tutorial (`/tutorial/*`) the canvas's empty state opens. Agents author through
 * `whiteboard mcp` (set up by `jocasta setup` with `DEV_REVIEW_HOME` only), which finds the same server as whiteboard's
 * Desktop instance `stable` (`<home>/review-desktop/instances/stable.json`).
 *
 * Installed, everything sits beside `jocasta.exe`: `node/node.exe` (Node 24, pinned in scripts/build.ts),
 * `whiteboard/` (whiteboard's runtime package as npm installs it, CLI at `dist/cli.js`, the Desktop server at
 * `dist/server/desktop-host.js`, the tutorial's built assets in `tutorial/`), `whiteboard-canvas/` (the canvas build)
 * and `editor.worker.js` (Monaco's worker, 0411). A source run uses `node` on PATH, the clone in `forks/whiteboard`
 * after its build (`bun run build` runs it) and Monaco's worker bundled in memory.
 */
import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join, normalize, sep } from "node:path";
import { freePort } from "../../shared/fsx.ts";
import { installDir, REPO } from "../../shared/install.ts";
import { dataDir } from "../../shared/paths.ts";
import { onPath } from "../../shared/process.ts";
import type { ServiceHealth } from "../protocol.ts";

/** whiteboard's telemetry off (its README; 0393). */
export const WHITEBOARD_ENV = { DO_NOT_TRACK: "1", DEV_FAST_REVIEW_TELEMETRY_DISABLED: "1" } as const;

/** The pinned whiteboard release (0393, 0417; plan appendix B): the canvas's `appVersion`. */
export const WHITEBOARD_VERSION = "0.2.4";

/** The repo's clone of whiteboard v0.2.4 (0417), relative to the repo root. */
const WHITEBOARD_FORK = join("forks", "whiteboard");

/** Where the page loads the canvas and Monaco's worker from (served by `server.ts`). */
export const CANVAS_PATH = "/whiteboard/canvas/";
export const WORKER_PATH = "/whiteboard/editor.worker.js";
/** The page's `GET` for the server's url and token and the canvas's files. */
export const CONNECTION_PATH = "/whiteboard/connection";

/** How to run whiteboard's CLI: a Node 24 binary and `cli.js`. */
export interface WhiteboardRuntime {
	node: string;
	cli: string;
}

/** whiteboard's state dir, `<data>/whiteboard` (0393). */
function whiteboardHome(): string {
	return join(dataDir(), "whiteboard");
}

/**
 * whiteboard's CLI: in the install (`<install dir>/whiteboard/dist/cli.js` with `<install dir>/node/node.exe`), or the
 * repo's built clone with `node` on PATH. Undefined when none is there (a source run before the clone is built).
 */
export function whiteboardRuntime(): WhiteboardRuntime | undefined {
	const dir = installDir();
	if (dir) {
		const cli = join(dir, "whiteboard", "dist", "cli.js");
		const node = join(dir, "node", process.platform === "win32" ? "node.exe" : "node");
		if (existsSync(cli) && existsSync(node)) return { node: normalize(node), cli: normalize(cli) };
	}
	const cli = join(REPO, WHITEBOARD_FORK, "packages", "review", "dist", "cli.js");
	return existsSync(cli) ? { node: onPath("node") ?? "node", cli } : undefined;
}

/** What the page needs: the server's loopback origin and token, and the canvas's built files. */
export interface WhiteboardConnection {
	url: string;
	token: string;
	/** The server's pid (the acceptance check lists its sockets). */
	pid: number;
}

export interface Whiteboard {
	/**
	 * The server's loopback origin, known before it listens (the page's CSP names it, 0393): the port this service
	 * chose, or the running server's when one was already up.
	 */
	origin(): string;
	/** Resolves once the server answers; rejects with why it couldn't start. */
	ready: Promise<WhiteboardConnection>;
	/** Whether the server is running and answers now, with why not (the dashboard's services row). */
	check(): Promise<Omit<ServiceHealth, "name">>;
	/** Stops the server this service started (Quit, 0357). */
	stop(): Promise<void>;
}

export interface WhiteboardOptions {
	/** whiteboard's CLI; found with `whiteboardRuntime()` by default; null when there's none (test seam). */
	runtime?: WhiteboardRuntime | null;
	/** whiteboard's state dir; `<data>/whiteboard` by default. */
	home?: string;
	/** How long the server may take to come up. */
	timeoutMs?: number;
	/** The loopback port; a free one by default. */
	port?: number;
}

interface Discovery {
	url: string;
	token: string;
	serverPid: number;
}

/** whiteboard's Desktop instance `stable` on this state: where `whiteboard mcp` finds it too. */
function readDiscovery(home: string): Discovery | undefined {
	try {
		const value = JSON.parse(
			readFileSync(join(home, "review-desktop", "instances", "stable.json"), "utf8"),
		) as Partial<Discovery>;
		const url = typeof value.url === "string" ? new URL(value.url) : undefined;
		if (url?.protocol !== "http:" || url.hostname !== "127.0.0.1") return undefined;
		if (typeof value.token !== "string" || typeof value.serverPid !== "number") return undefined;
		return { url: url.origin, token: value.token, serverPid: value.serverPid };
	} catch {
		return undefined;
	}
}

/** The recorded server answers `/health` with its token: one this service didn't start may still be running. */
async function healthy(discovery: Discovery): Promise<boolean> {
	try {
		const response = await fetch(`${discovery.url}/health`, {
			headers: { "x-review-token": discovery.token },
			signal: AbortSignal.timeout(1_500),
		});
		return response.ok;
	} catch {
		return false;
	}
}

/**
 * Starts `node <whiteboard>/dist/server/desktop-host.js` with `home` as its state (0393, 0419): its `ready` line on
 * stdout carries the url, token and pid, and `<home>/review-desktop/instances/stable.json` the same for
 * `whiteboard mcp`. A healthy server another Jocasta run left on the same state is used as is.
 */
export function startWhiteboard(options: WhiteboardOptions = {}): Whiteboard {
	const runtime = options.runtime === undefined ? whiteboardRuntime() : options.runtime;
	const home = options.home ?? whiteboardHome();
	const port = options.port ?? freePort();
	let origin = `http://127.0.0.1:${port}`;
	let child: ChildProcess | undefined;
	const ready = (async (): Promise<WhiteboardConnection> => {
		if (!runtime) {
			throw new Error(
				`whiteboard isn't built: run \`bun run build\` (it builds ${WHITEBOARD_FORK}), or install Jocasta's release`,
			);
		}
		mkdirSync(home, { recursive: true });
		// Another service's server on this state: whiteboard's Desktop server takes no lock, so ask it first.
		const existing = readDiscovery(home);
		if (existing && (await healthy(existing))) {
			origin = existing.url;
			return { url: existing.url, token: existing.token, pid: existing.serverPid };
		}
		const env: Record<string, string | undefined> = {
			...process.env,
			...WHITEBOARD_ENV,
			DEV_REVIEW_HOME: home,
			DEV_FAST_REVIEW_SERVER_PORT: String(port),
			// It hands this pid to agents as the Desktop app's.
			DEV_FAST_REVIEW_APP_PID: String(process.pid),
		};
		// A headless state dir or a dev checkout's identity would point it, and `whiteboard mcp`, elsewhere.
		delete env.DEV_REVIEW_SERVER_DIR;
		delete env.DEV_FAST_REVIEW_CHECKOUT;
		delete env.DEV_FAST_REVIEW_CHANNEL;
		const host = join(dirname(runtime.cli), "server", "desktop-host.js");
		const proc = spawn(runtime.node, [host], {
			cwd: home,
			env,
			stdio: ["ignore", "pipe", "pipe"],
			windowsHide: true,
		});
		child = proc;
		let output = "";
		const { promise, resolve: done, reject } = Promise.withResolvers<WhiteboardConnection>();
		const timer = setTimeout(
			() => reject(new Error(`whiteboard's server didn't start in time: ${output.trim()}`)),
			options.timeoutMs ?? 30_000,
		);
		proc.stdout?.setEncoding("utf8");
		proc.stderr?.setEncoding("utf8");
		proc.stderr?.on("data", (chunk: string) => {
			output = (output + chunk).slice(-4_000);
		});
		let buffered = "";
		proc.stdout?.on("data", (chunk: string) => {
			buffered += chunk;
			for (let i = buffered.indexOf("\n"); i >= 0; i = buffered.indexOf("\n")) {
				const line = buffered.slice(0, i).trim();
				buffered = buffered.slice(i + 1);
				let event: { event?: unknown };
				try {
					event = JSON.parse(line) as { event?: unknown };
				} catch {
					continue;
				}
				if (event.event !== "ready") continue;
				const discovery = readDiscovery(home);
				if (discovery) done({ url: discovery.url, token: discovery.token, pid: discovery.serverPid });
				else reject(new Error(`whiteboard's server is up but ${home} has no readable discovery file`));
			}
		});
		proc.on("error", (error) => reject(new Error(`whiteboard's server didn't start: ${error.message}`)));
		proc.on("close", (code) => {
			child = undefined;
			reject(new Error(`whiteboard's server exited ${code}: ${output.trim()}`));
		});
		try {
			return await promise;
		} finally {
			clearTimeout(timer);
		}
	})();
	// The service doesn't wait on it; the page asks, and gets the failure then.
	ready.catch(() => {});
	return {
		origin: () => origin,
		ready,
		check: async () => {
			const pending = Symbol("pending");
			const state = await Promise.race([
				ready.then(
					(c) => c,
					() => undefined,
				),
				Bun.sleep(0).then(() => pending),
			]);
			if (state === pending) return { state: "starting" };
			if (!state) return { state: "down", note: "not started" };
			const discovery = readDiscovery(home);
			return discovery && (await healthy(discovery)) ? { state: "up" } : { state: "down", note: "not answering" };
		},
		stop: async () => {
			const proc = child;
			if (!proc || proc.exitCode !== null) return;
			const exited = new Promise<void>((done) => proc.once("exit", () => done()));
			proc.kill();
			await exited;
		},
	};
}

/** The canvas build: `.vite/manifest.json` and `assets/`, beside `jocasta.exe` or in the built clone. */
export function canvasDir(): string | undefined {
	const dir = installDir();
	for (const candidate of [
		...(dir ? [join(dir, "whiteboard-canvas")] : []),
		join(REPO, WHITEBOARD_FORK, "packages", "review", "app", "dist", "desktop"),
	]) {
		if (existsSync(join(candidate, ".vite", "manifest.json"))) return candidate;
	}
	return undefined;
}

/** The canvas entry's files, from vite's manifest, as URLs under `CANVAS_PATH`. */
export interface CanvasFiles {
	script: string;
	styles: string[];
	wasm: string;
}

export function canvasFiles(dir: string): CanvasFiles {
	const manifest = JSON.parse(readFileSync(join(dir, ".vite", "manifest.json"), "utf8")) as Record<
		string,
		{ file: string; isEntry?: boolean; name?: string; css?: string[]; assets?: string[] }
	>;
	const entry = Object.values(manifest).find((chunk) => chunk.isEntry && chunk.name === "canvas");
	const wasm = entry?.assets?.find((file) => file.endsWith(".wasm"));
	if (!entry || !wasm) throw new Error(`${dir}: the canvas manifest has no canvas entry with its wasm`);
	return {
		script: CANVAS_PATH + entry.file,
		styles: (entry.css ?? []).map((file) => CANVAS_PATH + file),
		wasm: CANVAS_PATH + wasm,
	};
}

const TYPES: Record<string, string> = {
	".js": "text/javascript; charset=utf-8",
	".css": "text/css; charset=utf-8",
	".wasm": "application/wasm",
	".woff2": "font/woff2",
	".svg": "image/svg+xml",
	".png": "image/png",
};

/**
 * A file of the canvas build under `CANVAS_PATH`; undefined for anything outside `assets/`. vite names every file
 * there by its content, so the window keeps it for a year (PM-3).
 */
export function canvasAsset(dir: string, pathname: string): Response | undefined {
	const rel = decodeURIComponent(pathname.slice(CANVAS_PATH.length));
	const root = join(dir, "assets");
	const path = normalize(join(dir, rel));
	if (!path.startsWith(root + sep) || !existsSync(path)) return undefined;
	const type = TYPES[path.slice(path.lastIndexOf("."))] ?? "application/octet-stream";
	return new Response(Bun.file(path), {
		headers: { "content-type": type, "cache-control": "public, max-age=31536000, immutable" },
	});
}

/**
 * Monaco's editor worker (0411), served at `WORKER_PATH`: built beside `jocasta.exe` by scripts/build.ts, or bundled
 * once in memory in a source run.
 */
let worker: Promise<Blob | undefined> | undefined;
export async function monacoWorker(): Promise<Response | undefined> {
	worker ??= (async () => {
		const dir = installDir();
		if (dir && existsSync(join(dir, "editor.worker.js"))) return Bun.file(join(dir, "editor.worker.js"));
		const entry = join(REPO, "node_modules", "monaco-editor", "esm", "vs", "editor", "editor.worker.js");
		if (!existsSync(entry)) return undefined;
		const result = await Bun.build({ entrypoints: [entry], target: "browser", minify: true, format: "esm" });
		return result.outputs.find((output) => output.path.endsWith(".js"));
	})();
	const body = await worker;
	return body && new Response(body, { headers: { "content-type": "text/javascript; charset=utf-8" } });
}
