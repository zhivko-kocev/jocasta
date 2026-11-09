import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
	answerQuestion,
	holdQuestion,
	postAction,
	questionView,
	serviceAt,
	stateFeed,
} from "../../src/frontier/client.ts";
import { POPUP_READER_HEADER } from "../../src/frontier/protocol.ts";
import { removeTree } from "../../src/shared/fsx.ts";
import { NOT_RUNNING, writeServiceFile } from "../../src/shared/service-file.ts";

const TOKEN = "t0ken";
let saved: string | undefined;
let dir = "";
let server: { stop(force?: boolean): unknown; port?: number } | undefined;

beforeEach(() => {
	saved = process.env.JOCASTA_DATA_DIR;
	dir = mkdtempSync(join(tmpdir(), "jocasta-client-"));
	process.env.JOCASTA_DATA_DIR = dir;
});
afterEach(() => {
	server?.stop(true);
	server = undefined;
	if (saved === undefined) delete process.env.JOCASTA_DATA_DIR;
	else process.env.JOCASTA_DATA_DIR = saved;
	removeTree(dir);
});

/** A stand-in service on a free port, named in `jocasta-service.json` with this process's (live) pid. */
function serve(fetch: (request: Request) => Response | Promise<Response> | undefined): void {
	server = Bun.serve({
		port: 0,
		fetch(request, srv) {
			if (new URL(request.url).pathname === "/ws")
				return srv.upgrade(request) ? undefined : new Response("", { status: 400 });
			return fetch(request);
		},
		websocket: { message() {} },
	});
	writeServiceFile({ port: server.port ?? 0, token: TOKEN, pid: process.pid });
}

test("a jocasta-service.json whose process is gone means no service: the one not-running text (0581)", async () => {
	writeServiceFile({ port: 1, token: TOKEN, pid: 2 ** 31 - 2 });
	expect(serviceAt()).toBeUndefined();
	expect(await postAction("finish", "w1:p1")).toBe("Jocasta isn't running; start it with jocasta.");
	expect(await questionView("ABCDEF")).toBe(NOT_RUNNING);
});

test("a jocasta-service.json naming a live process with nothing listening on its port is the same text (0581 UX-13)", async () => {
	const closed = Bun.serve({ port: 0, fetch: () => new Response() });
	const port = closed.port as number;
	closed.stop(true);
	writeServiceFile({ port, token: TOKEN, pid: process.pid });
	expect(serviceAt()).toBeDefined();
	expect(await postAction("finish", "w1:p1")).toBe(NOT_RUNNING);
	expect(await questionView("ABCDEF")).toBe(NOT_RUNNING);
	const view = { title: "t", message: "m", choices: ["Deny"], fallback: "Deny" };
	expect(await answerQuestion({ code: "ABCDEF", view, reader: "r" }, { choice: "Deny" })).toEqual({
		kind: "failed",
		text: NOT_RUNNING,
	});
});

test("a question's one reader keeps the reader secret and sends it on the hold and the answer; the answer's replies (0578)", async () => {
	const seen: { path: string; reader: string | null }[] = [];
	let replies = [409, 409, 410];
	serve((request) => {
		const path = new URL(request.url).pathname;
		seen.push({ path, reader: request.headers.get(POPUP_READER_HEADER) });
		if (request.method === "GET" && path.endsWith("/hold")) return new Response("");
		if (request.method === "GET") {
			return Response.json(
				{ title: "Jocasta: Allow or Deny", message: "m", choices: ["Allow", "Deny"], fallback: "Deny", code: "KQX" },
				{ headers: { [POPUP_READER_HEADER]: "s3cret" } },
			);
		}
		const status = replies.shift() ?? 204;
		if (status === 409) return Response.json({ left: replies.length === 1 ? 1 : 2 }, { status });
		return new Response(null, { status });
	});
	const question = await questionView("ABCDEF");
	if (typeof question === "string") throw new Error(question);
	expect(question.reader).toBe("s3cret");
	expect(await answerQuestion(question, { choice: "Allow", typed: "AAA" })).toEqual({ kind: "wrong", left: 2 });
	expect(await answerQuestion(question, { choice: "Allow", typed: "AAA" })).toEqual({ kind: "wrong", left: 1 });
	expect(await answerQuestion(question, { choice: "Allow", typed: "AAA" })).toEqual({ kind: "exhausted" });
	replies = [];
	expect(await answerQuestion(question, { choice: "Deny" })).toEqual({ kind: "taken" });
	const { promise: ended, resolve } = Promise.withResolvers<void>();
	holdQuestion(question, resolve);
	await ended;
	expect(seen[0]).toEqual({ path: "/api/popup/ABCDEF", reader: null });
	expect(seen.slice(1).every((s) => s.reader === "s3cret")).toBe(true);
	expect(seen.at(-1)?.path).toBe("/api/popup/ABCDEF/hold");
});

test("the state feed: send says whether it went; once closed, no more calls (FE-3, FE-5)", async () => {
	serve(() => new Response("", { status: 404 }));
	const ups: boolean[] = [];
	const { promise: open, resolve } = Promise.withResolvers<void>();
	const feed = stateFeed(
		() => {},
		(up) => {
			ups.push(up);
			if (up) resolve();
		},
	);
	await open;
	expect(feed.send({ type: "pane.focus", paneId: "w1:p1" })).toBe(true);
	feed.close();
	expect(feed.send({ type: "pane.focus", paneId: "w1:p1" })).toBe(false);
	// The socket's own close event runs in the client after this; no signal of it reaches the test, so give it a moment.
	await Bun.sleep(100);
	expect(ups).toEqual([true]);
});
