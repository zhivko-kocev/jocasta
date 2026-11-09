import { createConnection } from "node:net";
import { herdrSocketPath } from "../../src/frontier/server/herdr.ts";

/**
 * One request to a herdr session's socket, as `herdr.ts` makes them (newline-delimited `{id, method, params}`, the
 * answer echoes `id`), for what the service's client keeps private: `pane.process_info`. Never starts a server.
 */
export async function herdrRequest<T>(session: string, method: string, params: object, timeoutMs = 10_000): Promise<T> {
	const id = `bench_${Date.now()}_${Math.random().toString(36).slice(2)}`;
	const answer = Promise.withResolvers<T>();
	const socket = createConnection(herdrSocketPath(session));
	let buffer = "";
	const timer = setTimeout(
		() => answer.reject(new Error(`herdr didn't answer ${method} in ${timeoutMs} ms`)),
		timeoutMs,
	);
	socket.on("connect", () => socket.write(`${JSON.stringify({ id, method, params })}\n`));
	socket.on("error", (error) => answer.reject(error));
	socket.on("close", () => answer.reject(new Error(`herdr closed the connection before answering ${method}`)));
	socket.on("data", (chunk) => {
		buffer += chunk.toString("utf8");
		for (let at = buffer.indexOf("\n"); at >= 0; at = buffer.indexOf("\n")) {
			const line = buffer.slice(0, at);
			buffer = buffer.slice(at + 1);
			let response: { id?: string; result?: T; error?: { message?: string } };
			try {
				response = JSON.parse(line);
			} catch {
				continue;
			}
			if (response.id !== id && response.id !== "") continue;
			if (response.error) answer.reject(new Error(response.error.message ?? `herdr error on ${method}`));
			else answer.resolve(response.result as T);
		}
	});
	try {
		return await answer.promise;
	} finally {
		clearTimeout(timer);
		socket.destroy();
	}
}

/** The process herdr started for a pane (`pane.process_info`'s `shell_pid`), or null. */
export async function paneShellPid(session: string, paneId: string): Promise<number | null> {
	try {
		const { process_info: info } = await herdrRequest<{ process_info: { shell_pid?: number | null } }>(
			session,
			"pane.process_info",
			{ pane_id: paneId },
		);
		return info.shell_pid ?? null;
	} catch {
		return null;
	}
}
