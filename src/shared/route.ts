// The `jocasta` command's words, kept apart from what each runs so `entry.ts` loads only the one it routes to (PM-11).
import { PLUGIN_ACTIONS, type PluginAction } from "../frontier/protocol.ts";

/**
 * The `jocasta` command's words (0387, 0467, 0468, 0472, 0544): open the work surface, `setup [--refresh <file>]`,
 * `uninstall`, `diagnose`, `mcp [--boss]`, `project …`, `thread …`, `worktree …`, `--version`; and, not listed in the
 * usage, `service`, the Jocasta service bare `jocasta` starts as its own process (0552), and `plugin
 * today|ask|startup|action <id>`, what the `jocasta` herdr plugin runs (0553).
 */
export type Route =
	| { role: "surface" }
	| { role: "service" }
	| { role: "setup"; refresh?: string }
	| { role: "uninstall" }
	| { role: "diagnose" }
	| { role: "mcp"; boss: boolean }
	| { role: "project"; args: string[] }
	| { role: "thread"; args: string[] }
	| { role: "worktree"; args: string[] }
	| { role: "plugin"; part: "today" | "ask" | "startup" }
	| { role: "plugin"; part: "action"; action: PluginAction }
	| { role: "version" }
	| { role: "help" }
	| { role: "usage" };

export const USAGE = [
	"usage: jocasta                  open the work surface (herdr session `jocasta`; starts the Jocasta service)",
	"       jocasta setup            show what Jocasta adds to omp, herdr, Windows Terminal and mise; write after `y`",
	"       jocasta setup --refresh herdr.toml | jocasta.json | mcp.json",
	"                               reset one Jocasta-owned file to the shipped version, keeping a backup",
	"       jocasta uninstall        remove what install and setup added, after `y`",
	"       jocasta diagnose         a read-only pane that explains what went wrong from Jocasta's log",
	"       jocasta mcp [--boss]     the MCP server omp starts (graph_search and report_result; the boss's tools with --boss)",
	"       jocasta project list | add <path> | remove <name|id>",
	'       jocasta thread new <project> "<brief>" [--title <t>] [--done-when "<cmd>"]...',
	'       jocasta thread list [<project>] | status <thread> | send <thread> "<text>" | stop <thread>',
	"       jocasta thread finish <thread> merge|pr",
	"       jocasta worktree list | remove <path>",
	"                               threads and leftover worktrees through the running service; changes ask you first",
	"       jocasta --version",
].join("\n");

/** Routes argv (0387); anything else is an invocation error (exit 2, 0140). */
export function route(args: string[]): Route {
	const [word, ...rest] = args;
	if (word === undefined) return { role: "surface" };
	if ((word === "--version" || word === "-v") && rest.length === 0) return { role: "version" };
	if ((word === "--help" || word === "-h") && rest.length === 0) return { role: "help" };
	if (word === "setup" && rest.length === 0) return { role: "setup" };
	if (word === "setup" && rest.length === 2 && rest[0] === "--refresh" && rest[1]) {
		return { role: "setup", refresh: rest[1] };
	}
	if (word === "uninstall" && rest.length === 0) return { role: "uninstall" };
	if (word === "service" && rest.length === 0) return { role: "service" };
	if (word === "diagnose" && rest.length === 0) return { role: "diagnose" };
	if (word === "mcp" && rest.length === 0) return { role: "mcp", boss: false };
	if (word === "mcp" && rest.length === 1 && rest[0] === "--boss") return { role: "mcp", boss: true };
	if (word === "project") return { role: "project", args: rest };
	if (word === "thread") return { role: "thread", args: rest };
	if (word === "worktree") return { role: "worktree", args: rest };
	if (word === "plugin") {
		const [part, action, ...more] = rest;
		if ((part === "today" || part === "ask" || part === "startup") && action === undefined)
			return { role: "plugin", part };
		const known = PLUGIN_ACTIONS.find((a) => a === action);
		if (part === "action" && known && more.length === 0) return { role: "plugin", part, action: known };
	}
	return { role: "usage" };
}
