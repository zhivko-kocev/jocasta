# 0689 Sleeping panes drawn by the shell; the footprint counts Jocasta's own processes

- Status: v1
- Kind: product
- Phase/slice: lightweight program, phase 6
- Date: 2026-11-25
- Amends: 0676 (how a sleeping pane is shown), 0674 (what the footprint counts)

## Question
Phase 6's own shell (0683) is over the footprint budget on processes and memory. What changes: the build, the metric, or the budget?

## Facts
- Measured on the packaged layout (bundled ConPTY, 3 runs, 2026-11-25): processes 14 (budget 7), idle RSS 190.2 MB (82.57), idle CPU 1.42–1.67 % (1.85), startup 42–48 ms (80.19), binary 21.75 MB (39.44).
- The tree: `jocasta.exe` 30.8 MB; 9 × `OpenConsole.exe`, the ConPTY host every terminal pane needs, about 11 MB each; 4 × `jocasta pane-sleep`, 12.5–16 MB each.
- The phase-0 budget measured the boss and 4 bare herdr workspaces (5 panes; its 4 fixture repos were identical, so one project). The ported bench starts 4 real threads, and each brings its project's sleeping pane: 9 panes.
- Idle CPU is about 0 between refreshes; the full refresh once a minute runs git for each thread (about 0.9 s of CPU for 4 threads).

## Options
| Option | Pros | Cons |
|---|---|---|
| (a) The shell draws a sleeping pane itself: no `pane-sleep` process and no ConPTY host while asleep | About 6 processes, about 86 MB | Borderline on memory; the console hosts still count |
| (b) (a), and the footprint counts only Jocasta's own processes, not the ConPTY hosts the OS runs for every terminal | 1 process (0683's aim), about 31 MB | A metric change |
| (c) Keep the build, re-baseline to the 9-pane run | Nothing to build | 14 processes and 190 MB become the ceiling |

## Recommendation
(b).

## Choice
His answer of 2026-11-25: (b).
- A sleeping pane is a pane of the shell with no process: its screen is the sleeper's (`<text>`, then `press any key to wake`), and a key in it asks the service to wake it (`wake` with the pane's key over the bridge, as the role did), showing `waking...` and, when the wake fails, why. Keys pressed while a wake is under way are ignored.
- The service asks for one with the argv `["jocasta:asleep", <text>]` (`surface::ASLEEP`) wherever it gave the `pane-sleep` role's argv; the internal role `pane-sleep` and `src/shared/pane_sleep.rs` are gone.
- The footprint's `processes` and `idle_rss_mb` count Jocasta's own processes in the service's tree, leaving out the agents' subtrees (as before) and the ConPTY hosts (`OpenConsole.exe`, `conhost.exe`). The budget is written again from the new count (it ratchets down from there).
- Kept by his word the same day: `/api/shutdown` stays removed (0683's built note).

## Reason
A sleeping pane runs nothing, so it should cost nothing; the console hosts belong to the OS and exist for any terminal.

## Consequences
The ported bench's 9-pane run is the footprint's run from now on.
