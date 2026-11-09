import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { connectHerdr, type Herdr } from "../../src/frontier/server/herdr.ts";
import { BenchUsageError } from "./args.ts";
import { herdrRequest, paneShellPid } from "./herdr.ts";
import { killTree } from "./proctree.ts";

/**
 * Starting a terminal agent in a real terminal for H5 and H6: Bun's PTY is POSIX-only, so on Windows each tool starts
 * in a pane of a separate bench herdr session (`--pty-session`, never his or the sandbox Jocasta's), and readiness is
 * herdr's `pane.wait_for_output` matching the tool's ready pattern on the visible screen.
 */

export const BENCH_TOOLS = ["omp", "claude", "opencode"] as const;
export type BenchTool = (typeof BENCH_TOOLS)[number];

/**
 * What each tool shows once its prompt takes input (`--ready tool=<regex>` replaces one). Seen in the VM on 2026-10-28:
 * omp 18.13.1's editor hint below its input box; unsigned opencode 1.18.22's command hint under its prompt; Claude
 * Code 2.1.296's footer, which reads "auto mode on (shift+tab to cycle)" in its default mode, auto, and "? for
 * shortcuts" in the others.
 */
export const DEFAULT_READY: Record<BenchTool, string> = {
	omp: "to change thinking effort",
	claude: "\\? for shortcuts|shift\\+tab to cycle",
	opencode: "ctrl\\+p",
};

/**
 * Claude Code asks whether to trust each folder it hasn't seen, and a bench pane can't answer: with Claude Code in the
 * tools, the panes work in `--cwd`, a folder trusted once, never in a fresh temp folder.
 */
export function requireTrustedCwd(tools: readonly string[], cwd: string | undefined): void {
	if (tools.includes("claude") && cwd === undefined)
		throw new BenchUsageError(
			"Claude Code asks to trust each new folder, and a bench pane can't answer: pass --cwd <a folder trusted once in Claude Code>",
		);
}

/** Tools the VM runs without a sign-in, and why (0506): timed starting and measured idle, never sent a prompt. */
export const UNSIGNED: Partial<Record<BenchTool, string>> = {
	opencode: "not signed in: its Anthropic sign-in takes only an API key (0506)",
};

/**
 * How a tool starts in a bench pane: the program a command line runs for its name (`Bun.which`, which follows PATHEXT
 * on Windows). herdr 0.9.5 picks the extensionless shim npm writes beside `opencode.cmd` for Git Bash, which Windows
 * can't start ("not a valid Win32 application"); and an npm `.cmd` shim that only runs an `.exe` is replaced by that
 * `.exe`, so the pane holds the tool alone, with no cmd.exe in its startup time or its memory.
 */
export function toolCommand(tool: BenchTool, which: (name: string) => string | null = Bun.which): string[] {
	const found = which(tool);
	if (found === null) throw new BenchUsageError(`${tool} is not on PATH`);
	if (!/\.(cmd|bat)$/i.test(found)) return [found];
	const target = /"%dp0%\\([^"]+\.exe)"/i.exec(readFileSync(found, "utf8"))?.[1];
	return target === undefined ? ["cmd.exe", "/d", "/c", found] : [join(dirname(found), target)];
}

/** `--ready omp=<regex>,claude=<regex>` over the defaults. */
export function readyPatterns(spec: string | undefined): Record<BenchTool, string> {
	const patterns = { ...DEFAULT_READY };
	for (const part of spec === undefined ? [] : spec.split(",")) {
		const at = part.indexOf("=");
		const tool = part.slice(0, at) as BenchTool;
		if (at < 1 || !BENCH_TOOLS.includes(tool)) throw new BenchUsageError(`--ready takes tool=<regex>, not "${part}"`);
		patterns[tool] = part.slice(at + 1);
	}
	return patterns;
}

const HIS_SESSIONS: Record<string, true> = { jocasta: true, jarvis: true };

export async function benchHerdr(session: string): Promise<Herdr> {
	if (HIS_SESSIONS[session.toLowerCase()]) throw new BenchUsageError(`--pty-session can't be ${session}`);
	return connectHerdr({ session });
}

export interface LaunchedPane {
	tabId: string;
	paneId: string;
}

/** A new tab in the bench workspace running `command` in `cwd`. */
export async function launchInPane(
	herdr: Herdr,
	workspaceId: string,
	command: string[],
	cwd: string,
): Promise<LaunchedPane> {
	return herdr.createTab(workspaceId, { title: `bench ${command[0] ?? ""}`, cwd, command });
}

/** ms until the pane's visible screen matches `pattern`, or null past `timeoutMs`; timed here, waited on by herdr. */
export async function waitReady(
	session: string,
	paneId: string,
	pattern: string,
	timeoutMs: number,
): Promise<number | null> {
	const started = performance.now();
	try {
		await herdrRequest(
			session,
			"pane.wait_for_output",
			{ pane_id: paneId, source: "visible", match: { type: "regex", value: pattern }, timeout_ms: timeoutMs },
			timeoutMs + 5_000,
		);
		return performance.now() - started;
	} catch {
		return null;
	}
}

/** Ends the pane's process tree and closes its tab. */
export async function closePane(session: string, pane: LaunchedPane): Promise<void> {
	const shell = await paneShellPid(session, pane.paneId);
	if (shell) killTree(shell);
	await herdrRequest(session, "tab.close", { tab_id: pane.tabId }).catch(() => undefined);
}
