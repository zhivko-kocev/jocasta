// A stand-in for one omp host in omp's local registry (omp 18.18.2 `src/collab/registry.ts`, protocol v1): a metadata
// file in `dir` and an endpoint (a named pipe on Windows, a socket elsewhere) answering one `snapshot` line for its
// bearer token, with the state a test sets.
import { randomBytes } from "node:crypto";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { join } from "node:path";
import { asObject } from "../../../src/shared/fsx.ts";

export interface FakeHostState {
	pid: number;
	cwd: string;
	busy: boolean | null;
	inputRequired: boolean;
}

export interface FakeHost {
	/** What the host answers from now on. */
	state: FakeHostState;
	/** How many snapshots it answered. */
	asked: number;
	close(): Promise<void>;
}

export async function fakeHost(
	dir: string,
	state: FakeHostState,
	token = randomBytes(8).toString("hex"),
): Promise<FakeHost> {
	mkdirSync(dir, { recursive: true });
	const entry = randomBytes(8).toString("hex");
	const endpoint = process.platform === "win32" ? `\\\\.\\pipe\\jocasta-test-${entry}` : join(dir, `${entry}.sock`);
	const host: FakeHost = {
		state,
		asked: 0,
		close: async () => {
			rmSync(meta, { force: true });
			const closed = Promise.withResolvers<void>();
			server.close(() => closed.resolve());
			await closed.promise;
		},
	};
	const server = createServer((socket) => {
		socket.setEncoding("utf8");
		let line = "";
		socket.on("data", (chunk: string) => {
			line += chunk;
			if (!line.includes("\n")) return;
			const request = asObject(JSON.parse(line.slice(0, line.indexOf("\n"))));
			if (request?.v !== 1 || request.token !== token || request.op !== "snapshot") {
				socket.end(`${JSON.stringify({ ok: false, v: 1, error: "authentication_failed" })}\n`);
				return;
			}
			host.asked++;
			const snapshot = {
				instanceId: entry,
				generation: 1,
				sessionId: "s",
				sessionName: null,
				model: null,
				startedAt: 0,
				participants: 1,
				relayConnected: true,
				access: "control",
				...host.state,
			};
			socket.end(`${JSON.stringify({ ok: true, v: 1, snapshot })}\n`);
		});
		socket.on("error", () => {});
	});
	const listening = Promise.withResolvers<void>();
	server.listen(endpoint, () => listening.resolve());
	await listening.promise;
	const meta = join(dir, `${entry}.json`);
	writeFileSync(meta, JSON.stringify({ version: 1, instanceId: entry, pid: state.pid, endpoint, createdAt: 0, token }));
	return host;
}
