# 0653 omp 19.1.0 is the pinned omp

- Status: v1 (built 2026-11-21; for his review)
- Kind: product, benchmark
- Phase/slice: fork build, phase 8
- Date: 2026-11-21

## Question
He updated omp to 19.1.0 on 2026-11-21 ("omp is updated to the latest, have that in mind"), a major version past the 18.22.2 that 0651 pinned. Does the pin follow, and what does Jocasta change to keep working on it?

## Facts
Verified 2026-11-21 against the published 19.1.0 packages and live runs (`docs/research/fork-build.md` choices 94–100).
- 19.0.0 and 19.1.0 break: `xd://` devices became tool commands run from bash (`propose`, `resolve`, `reject`, `report_issue`), the `tools.xdev*` settings became `tools.command*`, `tui.completeScrollback` is gone, MCP servers moved into one per-user omp daemon (`omp __omp_worker_daemon`, started detached by the first omp, a child of it by parent pid), `artifact://` became `local://N.log`, and composer predictions are on by default.
- Step A on 19.1.0: 643 settings; every key the snapshot and the overlays use is still registered with its old default; 16 added, 5 removed, one default changed (`composer.predictions`).
- omp 19 probes every stdio MCP server with `server/discover` before `initialize`; `jocasta mcp` answered `-32022` and omp failed every Jocasta tool.
- A Jocasta pane's process-tree end would also end the daemon when that pane's omp started it, taking every other omp's shared workers with it.
- A task-bench trial on 19.1.0 (`m-sonnet-medium`, `pallets__click-3642`) passed, its session read and priced by the ledger.

## Options
- **Pin 19.1.0 and fix the seams.** Pros: the omp he runs; his rule for development is the latest. Cons: H1's oracle can't run until omp's package sources are installed (For him 23).
- **Stay on 18.22.2.** Pros: the trials of 0651 ran there. Cons: he no longer runs it; Jocasta's MCP tools fail on his omp.

## Recommendation
Pin 19.1.0 and fix the seams.

## Choice
Built by his standing rule (the latest omp for development, 0623, 0651): the pin is 19.1.0 everywhere current. `jocasta mcp` answers the discovery probe as a handshake-era server, so each omp keeps its own Jocasta server with its pane's environment; plans are read from `propose`; omp's change journal (`changes.jsonl`) isn't read as a transcript; the boss manual names the tools as bash commands; H1's oracle stubs the new session field; the process-tree walks spare omp's daemon and what runs under it; the snapshot pins `composer.predictions: false`, the earlier behaviour.

## Reason
A paid run must not mix omp versions, and his omp is 19.1.0: the task model study runs on it.

## Consequences
- Amends 0651: the pin is 19.1.0. The snapshot's values were tried on 18.22.2 and are re-checked as keys on 19.1.0, not tried again.
- For his review (`fork-build.md` For him 21–25): the guard matching tool-command text, the daemon's working folder, H1 on 19.1.0, the new settings, composer predictions.
