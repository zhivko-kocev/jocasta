import { readServiceFile } from "../../src/shared/service-file.ts";
import { CLI_PATH } from "../../src/shared/thread-cli.ts";
import type { ToolReply } from "./sandbox.ts";

/**
 * Bench threads start and are read as `jocasta thread new` and `jocasta thread list` do (0576, 0544): `POST /api/cli`
 * with the service's token, which starts a thread without his Allow and needs no boss key. Threads still open in plan
 * mode and wait for his approval of their plan in their pane.
 */

/** One `jocasta thread|worktree` command against the running service (`<data>/jocasta-service.json`). */
export async function cliCall(command: string, args: Record<string, unknown>): Promise<ToolReply> {
	const running = readServiceFile();
	if (!running) throw new Error("no running service: <data>/jocasta-service.json is missing");
	const response = await fetch(`http://127.0.0.1:${running.port}${CLI_PATH}`, {
		method: "POST",
		headers: { authorization: `Bearer ${running.token}`, "content-type": "application/json" },
		body: JSON.stringify({ command, args }),
		// A start waits for omp to come up in its pane; the service bounds every step itself.
		timeout: false,
	});
	return (await response.json()) as ToolReply;
}

/** The branch a start's reply names (`Started <title> in <project> on branch <branch>…`), or undefined. */
export function startedBranch(reply: string): string | undefined {
	return / on branch (\S+?)(?:[.,]|$)/m.exec(reply)?.[1];
}

/** `jocasta thread new <project> "<brief>" --title <title>`; resolves the new thread's branch, throws on a refusal. */
export async function startThread(project: string, title: string, brief: string): Promise<string> {
	const reply = await cliCall("thread new", { project, title, brief });
	const branch = startedBranch(reply.text);
	if (reply.isError || branch === undefined) throw new Error(`thread new ${title}: ${reply.text}`);
	return branch;
}

/**
 * Prints `message` and waits for Enter on stdin: stopping a thread and removing its worktree ask him first (0484), so
 * the bench leaves closing its threads to the operator between runs.
 */
export async function waitForEnter(message: string): Promise<void> {
	console.log(message);
	for await (const _line of console) break;
}

export interface ThreadLine {
	title: string;
	branch: string;
	state: string;
	overlaps: string[];
	/** The outcome of the report it handed in for its current task (`· reported done`), or null. */
	reported: string | null;
}

/**
 * One `jocasta thread list` line: `<project>/<title> [<branch>] <state>[ — …][ · overlaps <title>: files; …][ · reported
 * <outcome>]`; null for any other line (its indented ledger line, "No threads.").
 */
export function parseThreadLine(line: string): ThreadLine | null {
	const match =
		/^[^/\s][^/]*\/(.+?) \[([^\]]+)\] (done|plan ready|awaiting you|idle|working|blocked|unknown)(.*)$/.exec(
			line.trimEnd(),
		);
	if (!match) return null;
	let rest = match[4] ?? "";
	const report = / · reported (\S+)$/.exec(rest);
	if (report) rest = rest.slice(0, report.index);
	const overlapsAt = rest.indexOf(" · overlaps ");
	const overlaps =
		overlapsAt < 0
			? []
			: rest
					.slice(overlapsAt + " · overlaps ".length)
					.split("; ")
					.map((part) => part.slice(0, part.lastIndexOf(": ")).trim())
					.filter((title) => title !== "");
	return {
		title: match[1] ?? "",
		branch: match[2] ?? "",
		state: match[3] ?? "",
		overlaps,
		reported: report?.[1] ?? null,
	};
}

/** `jocasta thread list [<project>]`, parsed; throws on a refusal. */
export async function listThreads(project?: string): Promise<ThreadLine[]> {
	const reply = await cliCall("thread list", project ? { project } : {});
	if (reply.isError) throw new Error(`thread list: ${reply.text}`);
	return reply.text
		.split("\n")
		.map(parseThreadLine)
		.filter((line): line is ThreadLine => line !== null);
}
