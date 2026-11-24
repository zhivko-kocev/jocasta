# 0676 Process diet: panes start on demand and sleep; a footprint that only goes down

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
One working omp holds 405 MB (947 MB private). How many omp processes stay alive while he has Jocasta open all day?

## Facts
Measured 2026-11-24:
- `crates/herdr/src` + `crates/herdr/crates`: 248,846 + 12,331 lines (plus Zig, ghostty-vt, vendored portable-pty).
- herdr at runtime (state-of-the-art §6): 16 MB with no panes, 27.6 MB with 16 shells, 0.16% of a core idle.
- One working omp: 405 MB working set, 947 MB private. Outside Jocasta's panes: `__omp_worker_mnemopi_embed` 1,033 MB, `__omp_worker_tiny_inference` 641 MB.
- `jocasta --version` starts in 70.4 ms median (the `pane-check` role pays this at every turn end). A named-pipe JSON round trip takes 39 µs median, 53 µs p95.
- `target/release/jocasta.exe`: 47,279,616 bytes. Whiteboard's `node.exe … desktop-host.js` (41 MB) outlived Jocasta's Stop by hours. One PR-review thread cost $70.25.
- `omp_ended` (`pane_launch.rs:620`) treats an ended omp as a crash; `PaneLaunch::respawn` and `--restore` exist (`Respawn` path, `pane_launch.rs:382–396`); `available_memory_bytes()` is `src/shared/pollers/machine.rs:33`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Every pane's omp lives all day (today) | No wake delay | About 0.4 GB each, idle; the machine's memory goes to idle agents |
| Panes start on demand, sleep after 15 idle minutes, a thread's omp ends while it awaits review, pressure sleep under the memory reserve | Memory follows work; sessions come back through `--restore` | A key press and a second before an old pane answers |
| Kill idle omp with no way back | Simplest | Loses the session |

## Recommendation
On demand and sleep, with a ratcheting budget.

## Choice
His answer of 2026-11-24:
- A new internal role `jocasta pane-sleep <text>` (`src/shared/pane_sleep.rs`) prints `<text>` and `press any key to wake`, reads one byte in raw mode, exits 0.
- Setting `service.paneIdleMinutes` (number, default 15, 0 = never).
- A project pane sleeps when the bridge has reported `agent_end` with `idle: true` and `jobs: 0`, no `input` is open, no question of that pane is open, and `paneIdleMinutes` have passed since its last `agent_end` or keypress-driven `agent_start`. A thread's pane sleeps at once when `ReviewFlow::ready` finishes under the same idle conditions. The boss never sleeps.
- To sleep: remember the session file (from `hello.session`), send `shutdown`, wait up to 10 s for the pipe to close, respawn the pane with `jocasta pane-sleep "<title> · asleep"`, mark it `asleep` (persisted with the thread file). A sleep is not a crash in `omp_ended`.
- To wake: a key press (the sleeper's process ends), `thread_send`, a review fix or `thread_finish` respawn omp with `--restore <session>` (`--continue` when no session file is known). Prompts sent meanwhile wait in the bridge's hello queue.
- At start project panes open as `pane-sleep "<project> · press any key to start"`.
- A sleeping thread or project shows ` · asleep` after its state in `today`.
- Pressure sleep: when `available_memory_bytes()` is under `checks.memoryReserveGb`, the longest-idle pane that meets the idle conditions sleeps at once, one per tick, the focused pane and the boss excepted. A working omp is never ended for memory.
- Daemon workers: after one day with only Jocasta open, `mise run footprint`'s `"omp"` list is read; if `__omp_worker_*` processes appear, work stops and he is asked with the measured sizes (worker settings off in Jocasta's panes, or leave as is). No setting changes without his answer.
- The footprint budget (0674) only ratchets down.

## Reason
Only the boss's omp runs when nothing is working.

## Consequences
- Journal kinds `slept` and `woke` (0688).
- Pending threads (0687) count a sleeping thread as not running.
- Needs the bridge's `agent_end`, `input` and `jobs` (0675, 0684).
