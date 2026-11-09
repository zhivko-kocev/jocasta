import { rmSync } from "node:fs";
import { createServer, type Server, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

export interface FakeRequest {
	id: string;
	method: string;
	params: Record<string, unknown>;
}

type Answer = { result: unknown } | { error: { code: string; message: string } };

/**
 * A herdr socket stand-in. Like herdr's API server it reads one request line per connection, answers it from `handlers`
 * and closes the connection, except `events.subscribe`, which stays open for pushed events.
 */
export class FakeHerdr {
	readonly requests: FakeRequest[] = [];
	connections = 0;
	readonly subscriptions: { socket: Socket; request: FakeRequest }[] = [];
	readonly handlers: Record<string, (params: Record<string, unknown>) => Answer> = {
		ping: () => ({ result: { type: "pong", version: "0.9.4", protocol: 22 } }),
		"events.subscribe": () => ({ result: { type: "subscription_started" } }),
	};
	/** How many more requests of a method to take and close without answering, as herdr 0.9.5 sometimes does (0517). */
	readonly drops: Record<string, number> = {};
	private readonly sockets = new Set<Socket>();

	private constructor(
		readonly path: string,
		private readonly server: Server,
	) {}

	/** Listens on `path` when given (a server started again on the same socket), else on a fresh one. */
	static async start(path?: string): Promise<FakeHerdr> {
		const name = `jocasta-herdr-test-${process.pid}-${Math.random().toString(36).slice(2)}`;
		path ??= process.platform === "win32" ? `\\\\.\\pipe\\${name}` : join(tmpdir(), `${name}.sock`);
		const server = createServer();
		const fake = new FakeHerdr(path, server);
		server.on("connection", (socket) => fake.accept(socket));
		const listening = Promise.withResolvers<void>();
		server.listen(path, () => listening.resolve());
		await listening.promise;
		return fake;
	}

	private accept(socket: Socket): void {
		this.sockets.add(socket);
		this.connections++;
		socket.on("close", () => this.sockets.delete(socket));
		socket.on("error", () => {});
		socket.setEncoding("utf8");
		let buffer: string | undefined = "";
		socket.on("data", (chunk: string) => {
			if (buffer === undefined) return;
			buffer += chunk;
			const end = buffer.indexOf("\n");
			if (end < 0) return;
			const request = JSON.parse(buffer.slice(0, end)) as FakeRequest;
			buffer = undefined;
			this.requests.push(request);
			const drops = this.drops[request.method] ?? 0;
			if (drops > 0) {
				this.drops[request.method] = drops - 1;
				socket.destroy();
				return;
			}
			const handler = this.handlers[request.method];
			const answer = handler
				? handler(request.params)
				: { error: { code: "unknown_method", message: `no ${request.method}` } };
			const line = `${JSON.stringify({ id: request.id, ...answer })}\n`;
			if (request.method === "events.subscribe" && "result" in answer) {
				socket.write(line);
				this.subscriptions.push({ socket, request });
			} else socket.end(line);
		});
	}

	/** Pushes an event to every open subscription that has an entry for its pane. */
	push(event: string, data: { pane_id: string } & Record<string, unknown>): void {
		for (const { socket, request } of this.subscriptions) {
			const entries = request.params.subscriptions as { type: string; pane_id?: string }[];
			if (socket.destroyed || !entries.some((e) => e.type === event && e.pane_id === data.pane_id)) continue;
			socket.write(`${JSON.stringify({ event, data })}\n`);
		}
	}

	/** Sends `events_lost` to every open subscription and closes it, as herdr does for a slow reader. */
	loseEvents(): void {
		for (const { socket, request } of this.subscriptions) {
			if (socket.destroyed) continue;
			socket.end(`${JSON.stringify({ id: request.id, error: { code: "events_lost", message: "lost" } })}\n`);
		}
	}

	methods(): string[] {
		return this.requests.map((r) => r.method);
	}

	last(method: string): FakeRequest | undefined {
		return this.requests.findLast((r) => r.method === method);
	}

	stop(): void {
		for (const socket of this.sockets) socket.destroy();
		this.server.close();
		if (process.platform !== "win32") rmSync(this.path, { force: true });
	}
}
