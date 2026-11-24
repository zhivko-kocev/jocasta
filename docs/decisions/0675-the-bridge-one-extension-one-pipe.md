# 0675 The bridge: one omp extension and one local pipe

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Amends: 0386, 0389, 0536, 0561, 0617 and plan 5.3
- Date: 2026-11-24

## Question
How do Jocasta's service and a pane's omp talk, with less weight than `jocasta mcp`, the collab relay and hosts, the `pane-check` role and typed prompts?

## Facts
Measured 2026-11-24:
- `crates/herdr/src` + `crates/herdr/crates`: 248,846 + 12,331 lines (plus Zig, ghostty-vt, vendored portable-pty).
- herdr at runtime (state-of-the-art §6): 16 MB with no panes, 27.6 MB with 16 shells, 0.16% of a core idle.
- One working omp: 405 MB working set, 947 MB private. Outside Jocasta's panes: `__omp_worker_mnemopi_embed` 1,033 MB, `__omp_worker_tiny_inference` 641 MB.
- `jocasta --version` starts in 70.4 ms median (the `pane-check` role pays this at every turn end). A named-pipe JSON round trip takes 39 µs median, 53 µs p95.
- `target/release/jocasta.exe`: 47,279,616 bytes. Whiteboard's `node.exe … desktop-host.js` (41 MB) outlived Jocasta's Stop by hours. One PR-review thread cost $70.25.
- Today: MCP (`rmcp`, 1,331 lines in `src/mcp/`), the collab relay (`tokio-tungstenite`), host polling, a `pane-check` role started per turn end, prompts typed into the screen and the screen read back.
- Extension tools without `loadMode` mount as bash commands (read from omp 19.1.1's source, `extensions/types.ts:1041`, `tools/tool-commands.ts:79-103`; not yet run).

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep MCP, relay and per-turn role | Nothing to migrate | 70 ms process start each turn end, an MCP child per pane, a relay and polling, typed prompts that race the screen |
| One omp extension plus one local pipe (JSON lines) | 39 µs round trip; push events replace polling; prompts by `pi.sendUserMessage`; one place knows omp; tools mount as commands, so the model's tool list stays short | A new protocol to maintain; tied to omp's extension API (re-checked at every pin) |
| omp `--mode rpc` | A defined protocol | It has no interactive screen; Jocasta panes need omp's screen |

## Recommendation
One extension and one pipe.

## Choice
His answer of 2026-11-24: the bridge replaces `jocasta mcp`, the collab relay and hosts, the `pane-check` role and extension, screen-typed prompts and screen reading.
- Wire: JSON lines, UTF-8, `\n`-terminated, 4 MiB line cap, on the named pipe `\\.\pipe\jocasta-bridge-<port>` (Windows) or the Unix socket `${XDG_RUNTIME_DIR:-/tmp/jocasta-<uid>}/bridge-<port>.sock` (directory 0700, socket 0600); framing reuses `src/shared/line_rpc.rs`. `<port>` is the service port.
- Messages: `hello` (pane: `key`=`JOCASTA_PANE_KEY`, `pid`, `session`, `harness`, `harness_version`, `caps`; CLI: `token`, `role: cli`), `welcome` (role boss/thread/project and the role's tools with schemas that live only in Rust), `call`/`result`/`cancel`, `stop`/`stop_result` (the turn-end check), `event` (`agent_start`, `agent_end {idle, jobs}`, `usage {tokens, percent}`, `todo {done, total}`, `input {open, kind}`, `caps`, `shutdown`), `prompt` (`deliver`: steer, followUp or nextTurn) with `ack`/`nack`, `abort`, `shutdown`, and `cli`/`push` for the CLI words and the push hook.
- Service side: `src/frontier/server/bridge.rs` (listener with `first_pipe_instance(true)` and `reject_remote_clients(true)`; a pane key resolves through `pane_of_key`; an unknown key or wrong token closes the connection). `Bridge::prompt` queues until the pane's hello and fails with `omp in <pane> didn't connect within 30 s`. Tools are the `TOOLS` table of `tools.rs`, filtered by role. `stop` calls `ReviewFlow::pane_check`. Events replace `hosts.rs`.
- omp side: `config/bridge-extension.ts` (strict ESM, one file, `node:net` only) replaces `config/pane-check-extension.ts` and is loaded with `-e` in every pane; `JOCASTA_BRIDGE` is set beside `JOCASTA_PANE_KEY`. On a lost pipe it reconnects every second; a tool call then throws `Jocasta's service isn't reachable; try again in a moment.`
- Step 1.0, before any Rust: a throwaway extension proves the mount rule live; if `probe_tool --help` doesn't run as a command, every bridge tool gets `loadMode: "essential"`.
- Deleted: `src/mcp/`, `relay.rs`, `hosts.rs`, `pane_check.rs` and the `pane-check` role, `rmcp` and `tokio-tungstenite`, the per-install `mcp.json` write; a setup migration removes the `jocasta` entry from `~/.omp/agent/mcp.json` (his other entries stay). `/api/cli`, `/api/push`, `/api/pane-check` and `MCP_PATH` go from `door_routes.rs`; `/api/shutdown` and `/health` stay on HTTP. The `Herdr` trait loses `prompt`, `send_text`, `submit`, `screen`, `send_keys`, `stop`.

## Reason
Lighter and faster transport, and one seam to replace when omp2 comes.

## Consequences
- Amends 0386 and 0389 (memory and harness wiring through MCP), 0536 and 0561 (the relay), 0617 (the turn-end extension and role) and plan 5.3.
- The pin's seam re-check (0383) covers `config/bridge-extension.ts`; its event and method names join plan 5.5's list.
- `bridge_ms` (a `call` round trip) joins the footprint budget; it must be under 5 ms.
- Herdr's own omp hook stays until phase 6 (its sidebar reads it).
