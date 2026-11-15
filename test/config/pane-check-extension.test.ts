import { afterEach, expect, jest, test } from "bun:test";
import paneCheckExtension, { LIMIT_MS } from "../../config/pane-check-extension.ts";
import { PANE_CHECK_ASK_MS, PANE_CHECK_ENV, PANE_CHECK_WAIT_MS } from "../../src/shared/pane-check.ts";

/** omp 18.20.2's limit for an extension's handler (`extensibility/extensions/runner.ts`, `EXTENSION_HANDLER_TIMEOUT_MS`). */
const OMP_HANDLER_LIMIT_MS = 30_000;

type Handler = (event: { signal?: AbortSignal }) => Promise<{ continue: true; additionalContext: string } | undefined>;

/** The handler the extension registers for `session_stop`, driven with the role's argv in the environment. */
function handlerFor(command: string[] | undefined): Handler {
	if (command) process.env[PANE_CHECK_ENV] = JSON.stringify(command);
	else delete process.env[PANE_CHECK_ENV];
	const registered: [string, Handler][] = [];
	paneCheckExtension({ on: (event, handler) => void registered.push([event, handler]) });
	expect(registered.map(([event]) => event)).toEqual(["session_stop"]);
	return (registered[0] as [string, Handler])[1];
}

/** A stand-in for the role: Bun running `code`. */
const bun = (code: string) => [process.execPath, "-e", code];

afterEach(() => {
	delete process.env[PANE_CHECK_ENV];
	jest.useRealTimers();
});

test("the handler continues the turn with the text the role prints", async () => {
	expect(await handlerFor(bun("process.stdout.write('CHECK FAILED: lint\\n')"))({})).toEqual({
		continue: true,
		additionalContext: "CHECK FAILED: lint",
	});
});

test("the handler answers nothing, and never throws, when the role prints nothing, fails, can't start or no command is given", async () => {
	expect(await handlerFor(bun("void 0"))({})).toBeUndefined();
	expect(await handlerFor(bun("process.stdout.write('half'); process.exit(3)"))({})).toBeUndefined();
	expect(await handlerFor(["no-such-program-for-jocasta-tests"])({})).toBeUndefined();
	expect(await handlerFor(undefined)({})).toBeUndefined();
	const garbled = handlerFor(undefined);
	process.env[PANE_CHECK_ENV] = "not json";
	expect(await garbled({})).toBeUndefined();
});

test("the service's wait, the role's and the extension's nest under omp's limit for a handler", () => {
	expect(PANE_CHECK_WAIT_MS).toBeLessThan(PANE_CHECK_ASK_MS);
	expect(PANE_CHECK_ASK_MS).toBeLessThan(LIMIT_MS);
	expect(LIMIT_MS).toBeLessThan(OMP_HANDLER_LIMIT_MS);
});

test("the handler answers nothing when the role runs past its limit, and ends it", async () => {
	jest.useFakeTimers();
	const handler = handlerFor(bun("setTimeout(() => process.stdout.write('late'), 60000)"));
	const answered = handler({});
	let settled = false;
	void answered.then(() => {
		settled = true;
	});
	jest.advanceTimersByTime(LIMIT_MS - 1);
	await Promise.resolve();
	expect(settled).toBe(false);
	// At the extension's limit (under omp's 30 s): the child is ended and nothing is answered.
	jest.advanceTimersByTime(1);
	expect(await answered).toBeUndefined();
});

test("the handler answers nothing when omp aborts the turn's settle, and ends the role", async () => {
	const abort = new AbortController();
	const answered = handlerFor(bun("setTimeout(() => process.stdout.write('late'), 60000)"))({ signal: abort.signal });
	abort.abort();
	expect(await answered).toBeUndefined();
});
