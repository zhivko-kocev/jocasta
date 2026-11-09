# 0536 Jocasta runs a local collab relay for thread state; it joins as a full guest only if a spike shows it's worth keeping

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V5, V11, V12
- Date: 2026-11-08

## Question
The harness-OS layer (0532) needs to prompt, stop and observe omp threads that stay interactive in herdr panes. Through which channel?

## Facts
Verified 2026-11-08 in omp 18.18.2's installed source (`@oh-my-pi/pi-coding-agent/src`) and its own docs (`omp://collab.md`).
- Today Jocasta types into a pane through herdr (`agent.prompt`, a paste then Enter; `pane.send_keys esc` to stop; `src/frontier/server/herdr.ts`) and reads state and cost from the session file.
- `--mode rpc` and `rpc-ui` draw no TUI: they can't drive a thread he also sees and types into.
- Collab: the host is authoritative and guests never peer; payloads are AES-256-GCM sealed and the relay sees only `[peerId][sealed payload]` envelopes and JSON control messages (`protocol.ts:4-7`). A guest sends `hello`, `prompt`, `abort`, `agent-cmd`, `fetch-transcript` and `ui-response` (answering the host's dialogs) and receives entry, event, state, subagent and dialog frames; there is no separate steer frame. Its frames and encryption are omp internals (`protocol.ts`, `crypto.ts`), not a documented API.
- `collab.autoStart` (`off|view|control`, default off) hosts every interactive session as it starts; `collab.relayUrl` defaults to the public `my.omp.sh`, and plain `ws://` is accepted only for localhost. The production relay isn't distributed; omp's repo has only a development stand-in (`packages/collab-web/scripts/local-relay.ts`, `/r/<roomId>`).
- Rooms follow the session: `/new`, `/resume`, `/fork` and branching start a new room, and a link of an older generation fails with `stale_generation`.
- `omp collab list --json` reports per host, over a local named pipe, `busy`, `inputRequired`, model, cwd, pid, session and generation; omp's docs invite dashboards to build on it. A room publishes there once its relay connection is up. A failing relay is a dim status line; the session goes on.
- What herdr keeps under any relay: the panes, workspaces and tabs, agent state for the toasts (H2), ending process trees (0486), restore after a restart (H3), the sidebar, focus, and its typing as the fallback.

## Options
| Option | Pros | Cons |
|---|---|---|
| Relay for state; the full guest if worth it | State without file lag from a documented listing, no encryption code; the guest only if the spike shows it holds up | Input stays herdr typing until then |
| Full guest after a spike | Prompt, abort, answering asks and live events from Jocasta | Reimplements omp's internal frames and encryption, re-checked at every bump |
| herdr and the session file | Works today; no collab | Events lag the file; asks only in the pane |

## Recommendation
Relay for state; the full guest if worth it.

## Choice
- Jocasta's service runs a small localhost relay that forwards rooms (`/r/<roomId>`); pane overlays set `collab.autoStart: control` and the localhost `collab.relayUrl`.
- Thread state (working or idle, an ask waiting, model, generation) comes from `omp collab list --json`; input stays herdr typing (`agent.prompt`, `esc`).
- A Windows spike tries the full guest: join from `omp collab link`, prompt, abort, answer an `ask`, live events, rejoin after a room changes, and no connection to `my.omp.sh`. The guest is built only if the spike passes and keeping up with omp's frames looks worth it; the result is recorded either way.
- herdr stays the work surface.

## Reason
First "i like the recommended version but would herdr have value still?" (the full guest after a spike; herdr keeps every role but typing); then, shown that the production relay isn't distributed, that the guest reimplements omp's internals and that `omp collab list` gives state cheaply: "Relay for state; guest if worth it".

## Consequences
- The pane overlays gain the two collab settings, with the relay URL fixed to localhost; a pane whose relay URL isn't local is a bug that fails closed (the thread doesn't start).
- The collab listing and, if built, the guest protocol join the seams re-checked at each omp bump (0383, 0535).
- Plan appendix I gains the relay seam when the build lands.
- Amended by 0561: the spike confirmed the relay; state is read from each host's pipe snapshot, panes also get a localhost `share.serverUrl` and refuse to start unless the relay resolves to loopback, and no full guest is built.
