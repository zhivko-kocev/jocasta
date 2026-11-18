# 0641 Process trees end two ways: process-wrap for Jocasta's own programs, the snapshot walk for the rest

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
0639 chose process-wrap 10 to end child process trees. The port's first waves ended trees the TypeScript way instead, and process-wrap went unused. How does the Rust build end process trees?

## Facts
Verified 2026-11-15 in the port's code and in process-wrap 10.0.1's source.
- **`proctree.rs` walks a process snapshot,** as the TypeScript build does: windows-sys Toolhelp on Windows, `/proc` and `ps` elsewhere. Each process is matched by its start time. 592 Rust tests pass on it.
- **The walk has to stay.** Jocasta ends trees it never started: a hung service found through `service.lock` (0534), and the omp processes in herdr's panes. 0582 spares the herdr server inside a hung service's tree. A Job Object covers neither case.
- **A job catches orphans.** A grandchild whose parent already exited stays in its job, while a snapshot walk can miss it, because its parent id names a dead process. The TypeScript build had that gap (0486, 0490).
- **process-wrap's job kills on close only when asked.** `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is set only with its `KillOnDrop` wrapper (`windows.rs`, `make_job_object`). Elsewhere it uses process groups.

## Options
- **The snapshot walk only; process-wrap leaves `Cargo.toml`.**
  - Pros: one mechanism, as the TypeScript build has; fewer crates.
  - Cons: amends 0639; orphaned grandchildren stay as uncaught as today.
- **Both.** process-wrap jobs for the programs Jocasta starts (the checks, `run_program`, the hunk daemon); the walk for the rest.
  - Pros: catches orphaned grandchildren of its own programs.
  - Cons: two ways to end a tree, more code and tests, and behaviour no longer the TypeScript build's exactly.

## Recommendation
The snapshot walk only.

## Choice
Both:
- **process-wrap** puts the programs Jocasta starts in a Job Object on Windows (a process group elsewhere) and ends them through it. These are the checks, `run_program` and the hunk daemon.
- **The snapshot walk** stays for trees Jocasta didn't start and for 0582's spare.
- **Jocasta's exit ends no program** that the TypeScript build left running: the hunk daemon's job doesn't kill on close.

## Reason
His answer: `both`, against the recommendation.

## Consequences
- Amends 0639: process-wrap ends the trees of Jocasta's own programs, beside the hand-written walk, not instead of it.
- Ending one of Jocasta's own programs can now also end an orphaned grandchild the TypeScript build would have left running. The port's differences ledger (`docs/research/rust-port.md`) lists it as a deliberate difference.
