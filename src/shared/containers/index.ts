import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { configDir } from "../paths.ts";
import { runProgram } from "../process.ts";
import { pidAlive } from "../proctree.ts";

/** A Compose health check (0072); durations in Compose's syntax (`10s`). */
export interface Healthcheck {
	test: string[];
	interval: string;
	timeout: string;
	retries: number;
	startPeriod: string;
}

/** One Compose service of a resource. */
export interface ContainerService {
	name: string;
	/** An official image, pinned by digest (0074). */
	image?: string;
	/** Jocasta's own image: its build context's files by name, `Dockerfile` among them (0074). */
	build?: Readonly<Record<string, string>>;
	environment?: Readonly<Record<string, string>>;
	/** Host port bindings, always on 127.0.0.1. */
	ports?: readonly string[];
	/** `<named volume>:<path in the container>`. */
	volumes?: readonly string[];
	/** Services that must be healthy first. */
	dependsOn?: readonly string[];
	/**
	 * Run Docker's init as PID 1 (compose `init: true`), which passes the stop signal on: a process that is PID 1 and
	 * sets no SIGTERM handler ignores it, so `docker compose stop` would wait its timeout and kill it (137).
	 */
	init?: boolean;
	healthcheck: Healthcheck;
}

/** A headless resource (0070): its services start together on demand and stop after `idleMs` unused (0073). */
export interface Resource {
	name: string;
	services: readonly ContainerService[];
	idleMs: number;
	/**
	 * Whether a call's error means the running services don't match this process's compose environment (Graphiti's 401:
	 * another run's token, 0430). `use` then brings them up again, which recreates exactly the services whose
	 * environment differs (volumes untouched), and retries the call once.
	 */
	stale?: (error: unknown) => boolean;
}

export interface RunResult {
	code: number;
	stdout: string;
	stderr: string;
}

/** Runs `docker compose -f <file> -p <composeProject()> <args>` with `env` added to its environment (test seam). */
export type ComposeRun = (file: string, args: string[], env?: Record<string, string>) => Promise<RunResult>;

/**
 * The Compose project Jocasta's containers run under: `jocasta` for the config dir in his home, and
 * `jocasta-<hash of the dir>` for any other (`JOCASTA_CODING_AGENT_DIR`, 0098), so a run with its own config dir never
 * touches his containers or their data.
 */
export function composeProject(): string {
	const dir = resolve(configDir());
	const home = resolve(homedir(), ".jocasta", "agent");
	const key = process.platform === "win32" ? dir.toLowerCase() : dir;
	if (key === (process.platform === "win32" ? home.toLowerCase() : home)) return "jocasta";
	return `jocasta-${createHash("sha256").update(key).digest("hex").slice(0, 8)}`;
}

/** How long `up --wait` may take: the first start builds Jocasta's images. */
const UP_TIMEOUT_S = 1800;

const HEADER = "# Written by Jocasta (0072): one file for every container it runs. Changes here are overwritten.\n";

export class ContainerError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "ContainerError";
	}
}

/** `~/.jocasta/agent/compose.yaml` (0072). */
export function composeFile(): string {
	return join(configDir(), "compose.yaml");
}

/** Where a Jocasta image's build context is written: `~/.jocasta/agent/containers/<service>/`. */
function buildContextDir(service: string): string {
	return join(configDir(), "containers", service);
}

/**
 * Where processes record that they use a resource: `~/.jocasta/agent/containers/leases/<resource>/<pid>.json`. Every
 * Jocasta process that uses a container runs its own manager; the last one using a resource stops it, so one process
 * exiting never stops Graphiti under another (0357).
 */
function leaseDir(resource: string): string {
	return join(configDir(), "containers", "leases", resource);
}

/** One process's use of a resource. */
interface Lease {
	/** Epoch ms of its last use. */
	lastUse: number;
	/** A call is in flight. */
	inflight: boolean;
}

/** A built image's tag: its build context's hash, so a changed Dockerfile builds a new image. */
function builtImageTag(service: ContainerService): string | undefined {
	if (!service.build) return undefined;
	const hash = createHash("sha256");
	for (const name of Object.keys(service.build).sort()) hash.update(`${name}\0${service.build[name]}\0`);
	return `jocasta/${service.name}:${hash.digest("hex").slice(0, 12)}`;
}

/** The Compose document for `resources`. */
export function renderCompose(resources: readonly Resource[]): string {
	const services: Record<string, unknown> = {};
	const volumes: Record<string, Record<string, never>> = {};
	for (const service of resources.flatMap((r) => r.services)) {
		const { healthcheck: h } = service;
		const tag = builtImageTag(service);
		services[service.name] = {
			image: tag ?? service.image,
			...(tag ? { build: { context: buildContextDir(service.name) } } : {}),
			restart: "on-failure",
			...(service.init ? { init: true } : {}),
			...(service.environment ? { environment: { ...service.environment } } : {}),
			...(service.ports ? { ports: [...service.ports] } : {}),
			...(service.volumes ? { volumes: [...service.volumes] } : {}),
			...(service.dependsOn
				? { depends_on: Object.fromEntries(service.dependsOn.map((d) => [d, { condition: "service_healthy" }])) }
				: {}),
			healthcheck: {
				test: [...h.test],
				interval: h.interval,
				timeout: h.timeout,
				retries: h.retries,
				start_period: h.startPeriod,
			},
		};
		for (const volume of service.volumes ?? []) {
			const name = volume.split(":")[0];
			if (name && !name.includes("/") && !name.includes("\\")) volumes[name] = {};
		}
	}
	const doc = { name: composeProject(), services, ...(Object.keys(volumes).length > 0 ? { volumes } : {}) };
	return HEADER + Bun.YAML.stringify(doc, null, 2);
}

function writeIfChanged(file: string, text: string): void {
	if (existsSync(file) && readFileSync(file, "utf8") === text) return;
	mkdirSync(dirname(file), { recursive: true });
	writeFileSync(file, text);
}

/** `docker compose` on this machine (0071). */
const dockerCompose: ComposeRun = async (file, args, env) => {
	try {
		const { code, stdout, stderr } = await runProgram(
			["docker", "compose", "-f", file, "-p", composeProject(), ...args],
			{
				...(env ? { env: { ...process.env, ...env } } : {}),
			},
		);
		return { code: code ?? 1, stdout, stderr };
	} catch (error) {
		throw new ContainerError(`docker isn't available: ${(error as Error).message}`);
	}
};

/** Errors that mean the service isn't there (stopped by another Jocasta process, or crashed). */
export function isConnectionError(error: unknown): boolean {
	if (!error || typeof error !== "object" || !("code" in error) || typeof error.code !== "string") return false;
	return /ConnectionRefused|ECONNREFUSED|ECONNRESET|ConnectionClosed|FailedToOpenSocket/.test(error.code);
}

/** Where `dockerEngineRunning` looks (test seams): the platform, the environment, a path check and the home dir. */
export interface DockerEngineProbe {
	platform?: NodeJS.Platform;
	env?: Record<string, string | undefined>;
	exists?: (path: string) => boolean;
	home?: string;
}

/**
 * Whether Docker's engine runs now, checked without spawning anything (0444; it runs every 10 s): its named pipe on
 * Windows (`\\.\pipe\dockerDesktopLinuxEngine` or `\\.\pipe\docker_engine`, there only while it runs), its socket
 * elsewhere (`/var/run/docker.sock` or `~/.docker/run/docker.sock`). A `DOCKER_HOST` of `npipe://` or `unix://` is
 * checked instead; a `tcp://` one counts as running.
 */
export function dockerEngineRunning(probe: DockerEngineProbe = {}): boolean {
	const env = probe.env ?? process.env;
	const exists = probe.exists ?? existsSync;
	const host = env.DOCKER_HOST?.trim();
	if (host?.startsWith("tcp://")) return true;
	// `npipe:////./pipe/docker_engine` is `\\.\pipe\docker_engine`.
	if (host?.startsWith("npipe://")) return exists(host.slice("npipe://".length).replaceAll("/", "\\"));
	if (host?.startsWith("unix://")) return exists(host.slice("unix://".length));
	if ((probe.platform ?? process.platform) === "win32") {
		return exists("\\\\.\\pipe\\dockerDesktopLinuxEngine") || exists("\\\\.\\pipe\\docker_engine");
	}
	return exists("/var/run/docker.sock") || exists(join(probe.home ?? homedir(), ".docker", "run", "docker.sock"));
}

/** What `nativeLinuxDocker` reads (test seams) beyond `DockerEngineProbe`: a file's text and a path's real target. */
export interface DockerDesktopProbe extends DockerEngineProbe {
	readText?: (path: string) => string | undefined;
	realPath?: (path: string) => string;
}

/**
 * Whether Docker here is Linux's own engine rather than Docker Desktop (SEC-LOC-10, 0584), checked without spawning:
 * only Docker Desktop gives containers a `host.docker.internal` that reaches the host's 127.0.0.1, where Jocasta's model
 * endpoint listens. Docker Desktop on Linux is its `desktop-linux` context (`DOCKER_CONTEXT`, else `currentContext` in
 * `~/.docker/config.json`) or its socket under `~/.docker/desktop/`; WSL's Docker Desktop integration links
 * `/var/run/docker.sock` into `docker-desktop`. Windows and macOS are always Docker Desktop.
 */
export function nativeLinuxDocker(probe: DockerDesktopProbe = {}): boolean {
	if ((probe.platform ?? process.platform) !== "linux") return false;
	const env = probe.env ?? process.env;
	const home = probe.home ?? homedir();
	const readText =
		probe.readText ??
		((path: string) => {
			try {
				return readFileSync(path, "utf8");
			} catch {
				return undefined;
			}
		});
	const real =
		probe.realPath ??
		((path: string) => {
			try {
				return realpathSync(path);
			} catch {
				return path;
			}
		});
	const host = env.DOCKER_HOST?.trim();
	if (host) return !/docker-desktop|\.docker\/desktop\//.test(host);
	let context = env.DOCKER_CONTEXT?.trim();
	if (!context) {
		try {
			const config: unknown = JSON.parse(readText(join(home, ".docker", "config.json")) ?? "{}");
			if (config && typeof config === "object" && "currentContext" in config) {
				if (typeof config.currentContext === "string") context = config.currentContext;
			}
		} catch {
			// An unreadable config is the default context.
		}
	}
	if (context === "desktop-linux") return false;
	return !real("/var/run/docker.sock").includes("docker-desktop");
}

export interface ContainersOptions {
	run?: ComposeRun;
	file?: () => string;
	/** Called synchronously at process exit with the services this process left running and no other process uses. */
	onExit?: (file: string, services: string[]) => void;
	/** Added to every `docker compose` command's environment (Graphiti's per-run token, 0430). */
	env?: () => Record<string, string>;
	/** Test seams: this process's id, whether another is alive, and the clock. */
	pid?: number;
	alive?: (pid: number) => boolean;
	now?: () => number;
}

/**
 * The one module that owns `compose.yaml` (0072): it writes the file and the build contexts of Jocasta's images, starts a
 * resource's services when first needed and stops them after the resource's idle time (0073), unless another Jocasta
 * process still uses them (a lease in `leaseDir`); the last user stops them.
 */
export class Containers {
	private readonly run: ComposeRun;
	private readonly file: () => string;
	private readonly running = new Set<string>();
	private readonly starting = new Map<string, Promise<void>>();
	private readonly inflight = new Map<string, number>();
	private readonly timers = new Map<string, Timer>();
	private readonly pid: number;
	private readonly alive: (pid: number) => boolean;
	private readonly now: () => number;
	private readonly env: () => Record<string, string> | undefined;

	constructor(
		private readonly resources: readonly Resource[],
		options: ContainersOptions = {},
	) {
		this.run = options.run ?? dockerCompose;
		this.file = options.file ?? composeFile;
		this.pid = options.pid ?? process.pid;
		this.alive = options.alive ?? pidAlive;
		this.now = options.now ?? Date.now;
		this.env = options.env ?? (() => undefined);
		const onExit = options.onExit;
		if (onExit) {
			process.once("exit", () => {
				const left = [...this.running].filter((name) => {
					this.release(name);
					return !this.usedElsewhere(name);
				});
				const services = left.flatMap((name) => this.resource(name).services.map((s) => s.name));
				if (services.length > 0) onExit(this.file(), services);
			});
		}
	}

	/** Records this process's use of the resource for the others (best effort: a lease that can't be written is skipped). */
	private lease(name: string, inflight: boolean): void {
		try {
			mkdirSync(leaseDir(name), { recursive: true });
			writeFileSync(
				join(leaseDir(name), `${this.pid}.json`),
				JSON.stringify({ lastUse: this.now(), inflight } satisfies Lease),
			);
		} catch {}
	}

	private release(name: string): void {
		rmSync(join(leaseDir(name), `${this.pid}.json`), { force: true });
	}

	/**
	 * Whether another live process has a call in flight or used the resource within its idle time. Leases of processes
	 * that are gone are removed.
	 */
	private usedElsewhere(name: string): boolean {
		const dir = leaseDir(name);
		let files: string[];
		try {
			files = readdirSync(dir);
		} catch {
			return false;
		}
		const idleMs = this.resource(name).idleMs;
		let used = false;
		for (const file of files) {
			const pid = Number(file.replace(/\.json$/, ""));
			if (!Number.isInteger(pid) || pid === this.pid) continue;
			if (!this.alive(pid)) {
				rmSync(join(dir, file), { force: true });
				continue;
			}
			try {
				const lease = JSON.parse(readFileSync(join(dir, file), "utf8")) as Lease;
				if (lease.inflight || this.now() - lease.lastUse < idleMs) used = true;
			} catch {
				// Being rewritten by its process: in use.
				used = true;
			}
		}
		return used;
	}

	private resource(name: string): Resource {
		const resource = this.resources.find((r) => r.name === name);
		if (!resource) throw new ContainerError(`unknown container resource: ${name}`);
		return resource;
	}

	/** Writes `compose.yaml` and the build contexts when their content changed; returns the file. */
	write(): string {
		const file = this.file();
		for (const service of this.resources.flatMap((r) => r.services)) {
			for (const [name, text] of Object.entries(service.build ?? {})) {
				writeIfChanged(join(buildContextDir(service.name), name), text);
			}
		}
		writeIfChanged(file, renderCompose(this.resources));
		return file;
	}

	/** Starts the resource's services and waits until they are healthy; builds Jocasta's images the first time. */
	ensure(name: string): Promise<void> {
		const resource = this.resource(name);
		if (this.running.has(name)) return Promise.resolve();
		let pending = this.starting.get(name);
		if (!pending) {
			pending = (async () => {
				const file = this.write();
				const services = resource.services.map((s) => s.name);
				const result = await this.run(
					file,
					["up", "--detach", "--wait", "--wait-timeout", String(UP_TIMEOUT_S), ...services],
					this.env(),
				);
				if (result.code !== 0) {
					throw new ContainerError(
						`couldn't start ${name} (docker compose exited ${result.code}): ${result.stderr.trim().split("\n").slice(-5).join("\n")}`,
					);
				}
				this.running.add(name);
			})().finally(() => this.starting.delete(name));
			this.starting.set(name, pending);
		}
		return pending;
	}

	/**
	 * Runs `fn` with the resource up. The idle timer pauses while any call is in flight. If the service went away under
	 * it (another Jocasta process stopped it), or answered with an error the resource calls stale (another process
	 * recreated it with its own environment), the resource is brought up again with this process's environment and `fn`
	 * retried once.
	 */
	async use<T>(name: string, fn: () => Promise<T>): Promise<T> {
		const resource = this.resource(name);
		this.inflight.set(name, (this.inflight.get(name) ?? 0) + 1);
		clearTimeout(this.timers.get(name));
		this.lease(name, true);
		try {
			await this.ensure(name);
			try {
				return await fn();
			} catch (error) {
				if (!isConnectionError(error) && !resource.stale?.(error)) throw error;
				this.running.delete(name);
				await this.ensure(name);
				return await fn();
			}
		} finally {
			const left = (this.inflight.get(name) ?? 1) - 1;
			this.inflight.set(name, left);
			this.lease(name, left > 0);
			if (left === 0 && this.running.has(name)) {
				const timer = setTimeout(() => void this.stop(name).catch(() => {}), resource.idleMs);
				timer.unref?.();
				this.timers.set(name, timer);
			}
		}
	}

	/**
	 * This process is done with the resource: its services stop (keeping their volumes) unless another Jocasta process
	 * still uses them, which then stops them when it is done.
	 */
	async stop(name: string): Promise<void> {
		const resource = this.resource(name);
		clearTimeout(this.timers.get(name));
		this.timers.delete(name);
		if ((this.inflight.get(name) ?? 0) > 0) return;
		this.running.delete(name);
		this.release(name);
		if (this.usedElsewhere(name)) return;
		const result = await this.run(this.file(), ["stop", ...resource.services.map((s) => s.name)], this.env());
		if (result.code !== 0) throw new ContainerError(`couldn't stop ${name}: ${result.stderr.trim()}`);
	}

	/**
	 * Jocasta closes (0434): the resource's services stop now, keeping their volumes, whichever process started them and
	 * whoever else holds a lease; nothing runs when `compose.yaml` was never written (no container ever ran).
	 */
	async halt(name: string): Promise<void> {
		const resource = this.resource(name);
		clearTimeout(this.timers.get(name));
		this.timers.delete(name);
		this.running.delete(name);
		this.release(name);
		const file = this.file();
		if (!existsSync(file)) return;
		const result = await this.run(file, ["stop", ...resource.services.map((s) => s.name)], this.env());
		if (result.code !== 0) throw new ContainerError(`couldn't stop ${name}: ${result.stderr.trim()}`);
	}

	/** Whether this process started the resource and hasn't stopped it. */
	isRunning(name: string): boolean {
		return this.running.has(name);
	}

	/** Whether this process is starting the resource now: the dashboard's services row says "starting" (0444). */
	isStarting(name: string): boolean {
		return this.starting.has(name);
	}
}

/** At exit, stops what this process left running without holding the exit up (a detached `docker compose stop`). */
export function stopDetached(file: string, services: string[]): void {
	try {
		// No docker on PATH arrives as an 'error' event (ENOENT), never as a throw (LC-11).
		spawn("docker", ["compose", "-f", file, "-p", composeProject(), "stop", ...services], {
			detached: true,
			stdio: "ignore",
			windowsHide: true,
		})
			.on("error", () => {})
			.unref();
	} catch {
		// Docker went away; nothing is left to stop.
	}
}
