# 0544 Thread work without a model turn: `jocasta thread` and `worktree` commands, and Finish and Stop buttons

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V1, V11, V12
- Date: 2026-11-08

## Question
Starting, steering, stopping and finishing a thread is code in the service, but the only door to it is a boss model turn. Does Jocasta add doors that need no model?

## Facts
Verified 2026-11-08 (`src/frontier/server/service.ts`, `src/mcp/server.ts`, `src/shared/route.ts`).
- `thread_start` is code from validation to the brief: catalog sync, the overlay, trunk, the herdr workspace and the worktree on `feature/<slug>`, omp's launch, the brief sent with an Enter retry (`service.ts:2872-2980`); only the brief's wording needs a model.
- `thread_finish` (merge or PR), the review's orchestration, `thread_stop` (one Escape) and `worktree_remove` are code (`service.ts:3015-3280`, `:2482`, `:2587-2603`).
- `jocasta` has `setup`, `uninstall`, `diagnose`, `mcp` and `project`; no `thread` commands (`route.ts:12-42`).
- When the boss finishes a thread or removes a worktree it asks him first (`ASKS_HIM`, 0484).
- 0513 measures the boss through a CLI against its MCP tools after v1.
- Thread panes run commands as him, and every pane's `jocasta mcp` reaches the service with the per-launch token; a command can't tell him from a thread, which is why the boss's tools need the boss key (0484, F2: "any pane can claim the boss's id"). The companion's page authenticates with an HttpOnly cookie panes can't read (0484, "Checked and safe"). The guard's known bypasses are an accepted risk (0485).

## Options
| Option | Pros | Cons |
|---|---|---|
| CLI and companion buttons | No tokens when he knows what he wants, from a terminal or the app | More surface to keep in step with the boss's tools |
| CLI only | Scriptable; less UI | Finishing needs a terminal |
| Buttons only | One click | Not scriptable |

## Recommendation
CLI and companion buttons.

## Choice
The service's own functions get two doors that need no model, beside the boss's tools:
```
jocasta thread new <project> "<brief>" [--title <t>] [--done-when "<cmd>"]...
jocasta thread list [<project>]
jocasta thread status <thread>
jocasta thread send <thread> "<text>"
jocasta thread stop <thread>
jocasta thread finish <thread> merge|pr
jocasta worktree list
jocasta worktree remove <path>
```
and Finish (merge or PR) and Stop buttons on a thread's card in the companion. A button is his confirmation. From the CLI, `thread send`, `thread stop`, `thread finish` and `worktree remove` raise the companion's Allow window, as the boss's path does, and do nothing on Deny or no answer within 2 minutes; `thread new`, `thread list`, `thread status` and `worktree list` run without it (a new thread waits at its plan for him). The rules the tools enforce (no finish while working or awaiting him, a dirty worktree refused, a re-review when trunk moved) hold for every door.

## Reason
"CLI + companion buttons"; then, told that a thread could run the CLI as him: "Allow for changes".

## Consequences
- `thread` and `worktree` claim `jocasta` subcommands in plan appendix I.
- `thread new` writes the task file and `done_when` like `thread_start` (0540).
- 0513's measurement of the boss through a CLI can use these commands.
- Amended by 0551 and 0552: Finish and Stop become keys in the `jocasta` pane; the CLI's confirmations are answered there, not in a window.
- Amended by 0590: `jocasta thread new` takes `--heavy`, as `thread_start` takes `heavy`: the thread runs on his `slow` role.
