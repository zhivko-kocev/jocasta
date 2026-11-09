# 0576 The live benchmarks on the harness-OS build

- Status: v1
- Kind: benchmark
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
The review found that several live benchmark scripts (H2–H8) can't run on the harness-OS build as written, or measure something other than they say. How does each run on this build?

## Facts
Verified 2026-11-09 by the review (0573), report BEN.
- `requireSandbox` reads only `JOCASTA_DATA_DIR` and `JOCASTA_HERDR_SESSION` (`bench/lib/sandbox.ts:17-27`); a spike passed it and still got `composeProject(): jocasta`, his Graphiti project, whose container a sandbox's first use recreates with its own token.
- H4 plants with `addEpisodes` from the bench process (`bench/h4-isolation.ts:100-111`), which throws "Jocasta's model endpoint isn't listening yet" after bringing Graphiti up once.
- H2, H6 and H8 start threads with the boss's `thread_start`, which in the VM waits for an Allow no operator can type (`CONFIRM_WAIT_MS = 120_000`), then counts as Deny.
- K11's "layer off" has no switch in the build; the flag only labels the run (bench/README.md:154-156).
- H5 ends Jocasta's run on herdr's idle state and omp's on its screen pattern; H2's `events`/`tick` no longer vary the path (threads report through omp's host, polled every 1 s).
- H4 counts errored replies as checks and gives a bound without a positive control; H8's odd pairs (rename vs use in another file) can never register a conflict.

## Options
| Item | Options (pros / cons) |
|---|---|
| BEN-2 sandbox | A. require `JOCASTA_CODING_AGENT_DIR` (not his) and check the data dir (closes it / one more variable) · B. document only · C. a temp config dir for spawned processes (misses H4 and hand starts) |
| BEN-3 H4 planting | A. through the service: `report_result` with `decisions` from an `h4 plant` thread (decided path, 0539 / canary inside a "Decisions settled" episode) · B. a boss-key bench route (a product change) · C. the watcher plants from sessions (slow, may drop the token) |
| BEN-4 bench starts | A. `POST /api/cli` `thread new` (no Allow, no boss key) · B. operator focuses a bench pane first (easy to forget) · C. operator types the code (herdr can't read a popup) |
| BEN-5 layer off | A. test-only `JOCASTA_LAYER=off` (same-build comparison / a new variable) · B. the comparison build (confounded) · C. drop K11's off arm (no same-build figure) |
| BEN-10, 13, 14, 15 | A (same end point; `host`/`herdr`; honest bound; merged-tree tests) · B and C (rename or note only) |
| BEN-20, 21, 22 | A (names fixed; parity exits 2 on nothing; build named in results) · leave them |

## Recommendation
A for every item.

## Choice
- BEN-2: `requireSandbox` also requires `JOCASTA_CODING_AGENT_DIR`, refuses it when it is `~/.jocasta/agent` (compose project `jocasta`), and refuses a `JOCASTA_DATA_DIR` equal to the default; one `refuseHisSession()` is used by launch.ts. The README and plan.md:2208 name the three variables.
- BEN-3: H4 plants its canaries through the service: a dedicated `h4 plant` thread in project A hands in `report_result` with `decisions: [{question, choice, why}]`, which the service posts to A's graph (0539's path); never `addEpisodes` from the bench process.
- BEN-4: H2, H6 and H8 start threads as `jocasta thread new` does (`POST /api/cli` `thread new`: no Allow, no boss key, 0544) and read them as `jocasta thread list` does.
- BEN-5: a test-only switch `JOCASTA_LAYER=off` (like `JOCASTA_HERDR_SESSION`, 0441): thread panes get no `check`/`report_result` tools, no task-file prompt (the layer's parts) and no code index; `/health` reports the layer state, and H8 refuses a mismatch with its `--layer` flag.
- BEN-10: H5 ends Jocasta's run with the same screen pattern as omp alone, on the boss pane; herdr-idle is kept as `bossIdleMs`; `companionMs` becomes `serviceMs`.
- BEN-13: H2's conditions become `host` (the default: omp's collab host polled every 1 s) and `herdr` (the service falls back to herdr's events and tick); t1 is kept as a reference figure. The `herdr` condition runs with a sandbox service started with a test-only `JOCASTA_HOSTS=off`, which ignores omp's host registry; `/health` reports it, and H2 refuses a mismatch.
- BEN-14: H4 counts only answered queries; it gives a bound only when the positive control found canaries (else `inconclusive`); `--pane-a` is required; it scans `mnemopi.db` and `mnemopi.db-wal`.
- BEN-15: H8's odd pairs get `test/pair<k>.test.ts` in the fixture; a pair that merges cleanly has its merged tree checked out in a temp worktree and `bun test` run; a failure counts as a conflict found only at merge (`semanticFailures`).
- BEN-20: H3's fault `companion` becomes `service` in `--faults` and in rows, with a `"renamed": {"service": "companion"}` note in h3-recovery.json; the companion-era lines are fixed (H4 runs on his PC, K11's counters, omp 18.18.2), and `companion-feed.ts` becomes `service-feed.ts`.
- BEN-21: tokscale parity exits 2 when it compared nothing.
- BEN-22: result files carry `jocasta --version` and `jocastaSha256` of the `jocasta.exe` on PATH.

## Reason
"A" for each, BEN-20–22 ticked as low-stakes. BEN-3, BEN-4, BEN-5 and BEN-13 were asked again with plain explanations at his request, then answered "A". BEN-13's first mechanism (stopping the relay) was found not to unhost threads during the build: omp registers its host locally with the relay down, and nothing stops the relay. Asked again the same day, he chose "A test-only switch JOCASTA_HOSTS=off" over dropping the condition.

## Consequences
- Amends 0482 and 0502 (the sandbox's variables, H4's planting procedure), 0516 (no operator Allow in H2/H6/H8), 0517/0520 (H2's conditions), 0529 (H8's semantic pairs), 0538 (parity), 0543 (K11's off arm is `JOCASTA_LAYER=off`) and 0571.
- 0087 principle 8: `JOCASTA_LAYER` and `JOCASTA_HOSTS` are test-only switches, not shipped off switches, as 0441's session variable is.
- The plan's appendix I gains `JOCASTA_LAYER` and `JOCASTA_HOSTS` beside `JOCASTA_HERDR_SESSION`; result rows rename `companionMs` to `serviceMs` and the `companion` fault to `service` (h3-recovery.json notes the rename).
