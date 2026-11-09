# 0561 The local relay works: thread state comes from omp's host pipes, panes refuse to start unless the relay is loopback, and no full guest

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V4, V11
- Date: 2026-11-09

## Question
0536's spike: does a localhost collab relay give Jocasta each thread's busy and inputRequired, does it fail closed without ever reaching `my.omp.sh`, and is the full guest worth building?

## Facts
Observed 2026-11-09 with omp 18.18.2 under a throwaway profile, a 108-line Bun relay on 127.0.0.1:7499, a fake OpenAI-compatible model on localhost for real turns, and a 113-line prototype guest (`D:/jocasta-spikes/relay/`).
- With `collab.autoStart: control` and `collab.relayUrl: ws://127.0.0.1:7499`, an interactive omp registered with the relay 2.8 s after launch; RPC and print modes never host.
- `busy` and `inputRequired` come from a named pipe each host publishes (metadata in `~/.omp/run/collab-hosts/<id>.json`, a bearer token per entry): a direct snapshot takes 0–5 ms, `omp collab list --json` 0.68–0.75 s (omp's startup). Both flipped within 100 ms of each prompt, abort and `ask` dialog. A host publishes only after its first relay connection: relay down at start means no entry; relay lost later keeps the entry with `relayConnected: false`.
- With the overlay omp only dialled the configured URL; a down relay meant retries from 1 s to 60 s and a dim status line, never a fallback; DNS and socket samples showed no `omp.sh`. The setting's default is `wss://my.omp.sh`, so a pane started without the overlay would host publicly; `/join <bare link>`, `/share` (`share.serverUrl`), `omp clip` and `/skills` reach omp.sh whatever the relay setting.
- The prototype guest sent a prompt, an abort and an `ask` answer and received live events; its protocol is internal (`COLLAB_PROTO = 3`, refused on mismatch), the registry's (`v: 1`) is documented for dashboards.
- The relay: one Bun process, ~52 MB, idle between 15 s pings; no frames flow unless a guest joins.
- A profile isn't fully isolated: it still loaded his user-level MCP server and skills.

## Options
No choice for him: 0536 decided the relay, the fail-closed rule and that the guest is built only if the spike shows it's worth it; the spike shows how (0363).

## Recommendation
As the choice.

## Choice
- Jocasta's service runs the relay on 127.0.0.1 at one fixed port; every pane's overlay sets `collab.autoStart: control`, `collab.relayUrl: ws://127.0.0.1:<port>` and `share.serverUrl` to a localhost address.
- Before starting a pane, Jocasta resolves `collab.relayUrl` with that pane's overlays and refuses to start it unless the value is loopback.
- Thread state reads each host's pipe snapshot (filtered to Jocasta's panes by pid and cwd), not `omp collab list`; a pane without an entry is "unknown", never idle (0547: the session file then decides).
- No full guest: Jocasta keeps typing through herdr (0536), as reading state needs no guest.

## Reason
The spike's results; 0536's rule.

## Consequences
- Carries out 0536 and 0547.
- `/share`, `/join` and `omp clip` stay his own commands to the public service; Jocasta doesn't block them.
- The benchmark's own profiles must not load his user-level MCP servers and skills (checked when the arms are set up in the VM).
- Left on disk: `D:/jocasta-spikes/relay/`; an empty `~/.omp/run/collab-hosts/` (omp's own folder, recreated on demand). The spike profile `~/.omp/profiles/jocasta-spike-relay/` was deleted. No install, nothing running. During the first launch an Enter keypress on omp's setup wizard began a ChatGPT login (an OAuth link and a callback on localhost:1455); the process was killed and no login completed.
