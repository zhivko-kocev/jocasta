import { readFrontierFile } from "../frontier/server/frontier-file.ts";
import { UserError } from "./errors.ts";
import { asObject, pidAlive } from "./fsx.ts";

/**
 * `jocasta thread …` and `jocasta worktree …` (0544): doors to the running service's own thread functions (the ones the
 * boss's tools use) that need no model, over its localhost API with its token (`POST /api/cli`). The changes (`thread
 * send|stop|finish`, `worktree remove`) wait for his Allow in herdr's popup; the rest run at once.
 */

/** Where the service takes these commands: bearer token, body `{command, args}`, reply `{text, isError?}`. */
export const CLI_PATH = "/api/cli";

/** Exit codes: done; refused or failed (Deny and no answer included); bad usage (0140); Jocasta isn't running. */
export const EXIT = { done: 0, refused: 1, usage: 2, notRunning: 3 } as const;

export const THREAD_USAGE = [
	'usage: jocasta thread new <project> "<brief>" [--title <t>] [--done-when "<cmd>"]...',
	"       jocasta thread list [<project>]",
	"       jocasta thread status <thread>",
	'       jocasta thread send <thread> "<text>"',
	"       jocasta thread stop <thread>",
	"       jocasta thread finish <thread> merge|pr",
].join("\n");

export const WORKTREE_USAGE = "usage: jocasta worktree list | remove <path>";

/** One command for the service: its name (`thread new`) and arguments, as the boss's tools name them. */
export interface CliRequest {
	command: string;
	args: Record<string, unknown>;
	/** It waits for his Allow. */
	asks: boolean;
}

/** `jocasta thread|worktree <args>` as a request; bad usage is a `UserError` with exit 2 and the usage. */
export function cliRequest(word: "thread" | "worktree", args: string[]): CliRequest {
	const [verb, ...rest] = args;
	const usage = () => new UserError(word === "thread" ? THREAD_USAGE : WORKTREE_USAGE, EXIT.usage);
	const one = (key: string, asks: boolean): CliRequest => {
		const [value] = rest;
		if (rest.length !== 1 || !value?.trim()) throw usage();
		return { command: `${word} ${verb}`, args: { [key]: value }, asks };
	};
	if (word === "worktree") {
		if (verb === "list" && rest.length === 0) return { command: "worktree list", args: {}, asks: false };
		if (verb === "remove") return one("path", true);
		throw usage();
	}
	switch (verb) {
		case "new": {
			const positional: string[] = [];
			const doneWhen: string[] = [];
			let title: string | undefined;
			for (let i = 0; i < rest.length; i++) {
				const arg = rest[i] as string;
				if (arg === "--title" || arg === "--done-when") {
					const value = rest[++i];
					if (value === undefined || !value.trim()) throw usage();
					if (arg === "--title") {
						if (title !== undefined) throw usage();
						title = value;
					} else doneWhen.push(value);
				} else if (arg.startsWith("--")) throw usage();
				else positional.push(arg);
			}
			const [project, brief] = positional;
			if (positional.length !== 2 || !project?.trim() || !brief?.trim()) throw usage();
			return {
				command: "thread new",
				args: { project, brief, ...(title ? { title } : {}), ...(doneWhen.length > 0 ? { done_when: doneWhen } : {}) },
				asks: false,
			};
		}
		case "list": {
			if (rest.length > 1) throw usage();
			return { command: "thread list", args: rest[0] ? { project: rest[0] } : {}, asks: false };
		}
		case "status":
			return one("thread", false);
		case "stop":
			return one("thread", true);
		case "send": {
			const [thread, text] = rest;
			if (rest.length !== 2 || !thread?.trim() || !text?.trim()) throw usage();
			return { command: "thread send", args: { thread, text }, asks: true };
		}
		case "finish": {
			const [thread, how] = rest;
			if (rest.length !== 2 || !thread?.trim() || (how !== "merge" && how !== "pr")) throw usage();
			return { command: "thread finish", args: { thread, how }, asks: true };
		}
		default:
			throw usage();
	}
}

export interface CliDeps {
	/** The running service's port and token from `<data>/frontier.json`, or undefined when its process is gone. */
	service(): { port: number; token: string } | undefined;
	print(line: string): void;
	printError(line: string): void;
}

/** The service named by `<data>/frontier.json` while its process lives. */
export function runningService(): { port: number; token: string } | undefined {
	const file = readFrontierFile();
	return file && pidAlive(file.pid) ? { port: file.port, token: file.token } : undefined;
}

const NOT_RUNNING = "Jocasta isn't running; start it with `jocasta`.";

/** Runs one command against the service and prints its answer; resolves the exit code (`EXIT`). */
export async function runCliCommand(request: CliRequest, deps: CliDeps): Promise<number> {
	const address = deps.service();
	if (!address) {
		deps.printError(`jocasta: ${NOT_RUNNING}`);
		return EXIT.notRunning;
	}
	if (request.asks) deps.printError("jocasta: asking you first in herdr's popup (Allow with its code; 2 minutes)…");
	let response: Response;
	try {
		response = await fetch(`http://127.0.0.1:${address.port}${CLI_PATH}`, {
			method: "POST",
			headers: { authorization: `Bearer ${address.token}`, "content-type": "application/json" },
			body: JSON.stringify({ command: request.command, args: request.args }),
			// His Allow waits 2 minutes and a merge's hooks may take 10; the service bounds every step itself.
			timeout: false,
		});
	} catch (error) {
		const code = asObject(error)?.code;
		if (code === "ConnectionRefused" || code === "ECONNREFUSED") {
			deps.printError(`jocasta: ${NOT_RUNNING}`);
			return EXIT.notRunning;
		}
		deps.printError(`jocasta: Jocasta's service failed: ${error instanceof Error ? error.message : String(error)}`);
		return EXIT.refused;
	}
	let body: unknown;
	try {
		body = await response.json();
	} catch {
		body = undefined;
	}
	const text = asObject(body)?.text;
	if (typeof text !== "string") {
		deps.printError(`jocasta: Jocasta's service answered HTTP ${response.status} without a result`);
		return EXIT.refused;
	}
	if (!response.ok || asObject(body)?.isError === true) {
		deps.printError(`jocasta: ${text}`);
		return EXIT.refused;
	}
	deps.print(text);
	return EXIT.done;
}
