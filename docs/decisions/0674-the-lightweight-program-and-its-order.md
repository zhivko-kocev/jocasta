# 0674 The lightweight program, its order and its footprint budget

- Status: v1
- Kind: product, process
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
He asked on 2026-11-24 for a lighter, faster, token-cheaper Jocasta: no herdr bloat, no MCP, deterministic validation loops, deterministic animated HTML briefs for every process of his day, strong privacy, and only Jocasta open all day. Stack stays Rust; omp is the agent and Jocasta re-implements nothing omp does. In what order is it built, and how is progress kept honest?

## Facts
Measured 2026-11-24:
- `crates/herdr/src` + `crates/herdr/crates`: 248,846 + 12,331 lines (plus Zig, ghostty-vt, vendored portable-pty).
- herdr at runtime (state-of-the-art §6): 16 MB with no panes, 27.6 MB with 16 shells, 0.16% of a core idle.
- One working omp: 405 MB working set, 947 MB private. Outside Jocasta's panes: `__omp_worker_mnemopi_embed` 1,033 MB, `__omp_worker_tiny_inference` 641 MB.
- `jocasta --version` starts in 70.4 ms median (the `pane-check` role pays this at every turn end). A named-pipe JSON round trip takes 39 µs median, 53 µs p95.
- `target/release/jocasta.exe`: 47,279,616 bytes. Whiteboard's `node.exe … desktop-host.js` (41 MB) outlived Jocasta's Stop by hours. One PR-review thread cost $70.25.
- Reading: herdr is the code weight, omp processes the memory weight, MCP plus the relay plus the per-turn role the transport weight.
- `async-imap` 0.12.0 (2026-09-30) and `alacritty_terminal` 0.26.0 (2026-04-06) pass the 7-day rule.

## Options
| Option | Pros | Cons |
|---|---|---|
| Transport, processes, loops, briefs, day flows, then the surface (phases 0–6), a footprint budget that only ratchets down | Each phase removes weight the next depends on not having; the riskiest part (own shell) comes last and is gated by a spike; benchmarks run on the lightweight build | Long; the fork stays in the tree until phase 6 |
| Start with the own shell (drop herdr first) | Biggest code cut first | Highest risk first; the bridge and the process diet would have to be built twice, against herdr and then the new shell |
| Tune the fork only (settings, no removals) | Cheap, no risk | Leaves MCP, the relay, the per-turn role and the idle omp processes, the measured weight |

## Recommendation
Phases 0–6 in the order transport, processes, loops, briefs, day flows, surface.

## Choice
Approved on 2026-11-24, in the order of the plan `jocasta-lightweight-plan`:
- Phase 0: baseline and the footprint budget. Phase 1: the bridge (0675). Phase 2: process diet (0676). Phase 3: deterministic loops, stage setting, breakers, journal (0677, 0682, 0685, 0686, 0688). Phase 4: briefs, review notes, end of whiteboard and hunk (0678, 0679, 0681). Phase 5: day flows, thread graph, sources, privacy (0680, 0681, 0687). Phase 6: the surface (0683), gated by its spike.
- Phase 0 adds `bench/footprint.rs` (`jocasta-bench footprint [--write-budget]`) printing `{"processes", "idle_rss_mb", "idle_cpu_pct", "startup_ms", "binary_mb", "bridge_ms"}` for the boss and 4 panes on the bench's stub agent after 60 s idle; omp-named processes are listed under `"omp"` and never budgeted. `--write-budget` writes `bench/footprint-budget.json`; without it any number more than 5% over its budget prints `footprint: <key> <value> over budget <budget>` and exits 1 (a `null` budget is skipped). The mise task `footprint` is not part of `check`.
- Every phase ends with `mise run check` and `mise run footprint` green and one commit; when numbers fall the budget is rewritten (it only goes down).
- Rules: each decision's record is written before its code (0002); placeholder names only (`acme`, `client-a`, `example-org`).
- The benchmarks run after the program, on the lightweight build.

## Reason
His ask: lighter, faster, cheaper, and Jocasta the only app open all day.

## Consequences
- Records 0674–0688 hold the decisions; this record holds the order.
- The budget is a ratchet from phase 0's own measurement, so no target number is invented.
- The next slice is phase 0.
