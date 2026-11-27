# 0719 Settings from the 2026-12 trials

- Status: v1 (built 2026-11-27)
- Kind: product, benchmark
- Phase/slice: ultron 0.3 iteration, M3 T1
- Date: 2026-11-27
- Amends: 0651 (its trials re-run on omp 19.2.1)

## Question
omp moved from 18.22.2 (0651's trials) to 19.2.1. Which of its settings, new, newly live or already adopted, should Jocasta's snapshot, project overlay and boss overlay carry now?

## Facts
- T1 re-ran the settings trials on his Linux PC under his omp login: `jocasta-bench evals settings enumerate|replay|screen|confirm`, run folder `~/thesis/data/settings/linux-2026-11-27` (its `PROPOSAL.md`, `report-table.md`, `screen.json`, `confirm.json`). The harness ran from a worktree with `harness.patch` (brake $300, a chosen `trial-set.txt`, `JOCASTA_SETTINGS_BASE` for the boss replay); the patch stays outside `main`.
- Enumerate: 648 settings (P 162, J 32, F 83, Q 18, C 353), 582 arm values; 22 keys new since 18.22.2, 6 gone, 2 defaults changed (`composer.predictions` on, `eval.tools.enabled` off).
- Replay (no model): 495 inert, 87 live; 21 newly live, 9 of them with different requests. `todo.eager: preferred` is inert in the replay now.
- Screen: 16 arms over A1 (the 9, a recheck of `todo.eager: preferred`, and the plan's `advisor-end`, `advisor-every3`, `think-medium`, `eval-off`, `lsp-diag-off`, `cachewarm-idle`, now `evals/variants/*.yaml`; `advisor-off` is A1 itself, 0619), base 12 cases × 5. Every one of 380 trials passed. Keep rule: the harness's `Standing::wins` (not out, and the cost or wall-time interval's upper bound below zero at an equal pass count).
- Confirm: `todo.eager: preferred` against the base, 12 × 5 interleaved: $0.0732 vs $0.0807 a trial (cost × 0.84, 95 % interval −$0.0098 … −$0.0048), time × 1.03 (interval across zero), tokens × 0.99.
- Spend: $30.57 ($29.13 on the harness ledger, $1.44 of advisor calls the ledger doesn't count). The $300 brake was not approached.
- Screen table (base 60/60, $0.0841 and 14.5 s a trial; "× base" are the harness's mean per-case ratios; 4-case arms ran the 4 cheapest cases):

| Arm | Cases | Passed | Cost a trial | Time a trial | Cost × base | Time × base | Cost Δ 95 % (USD/trial) | Time Δ 95 % (s) | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| `todo.eager: preferred` (recheck of 0651) | 12 | 24/24 | $0.0734 | 14.6 s | 0.81 | 1.00 | −0.0145 … −0.0074 | −0.6 … +0.8 | **wins** (C) |
| `think-medium` | 12 | 24/24 | $0.0784 | 13.7 s | 0.94 | 0.94 | −0.0104 … −0.0015 | −1.5 … −0.1 | **wins** (Q: reported, not adopted by the harness) |
| `edit.mode: magic` | 12 | 24/24 | $0.0766 | 13.6 s | 1.07 | 0.94 | −0.0174 … +0.0044 | −2.2 … +0.4 | no |
| `disassembler.backend: binja` | 12 | 24/24 | $0.0834 | 14.3 s | 1.16 | 0.97 | −0.0078 … +0.0095 | −1.0 … +0.8 | no |
| `task.crossInstance: false` | 12 | 12/12 | $0.0837 | 14.0 s | 1.13 | 0.98 | −0.0081 … +0.0086 | −1.4 … +0.6 | no |
| `tools.commands: false` | 12 | 24/24 | $0.0898 | 14.3 s | 1.30 | 0.98 | −0.0034 … +0.0182 | −1.5 … +1.2 | no |
| `advisor-end` (J2) | 12 | 24/24 | $0.0791 + $0.056 advisor | 18.6 s | 0.99 (main only) | 1.31 | −0.0176 … +0.0040 | +2.6 … +6.0 | no (slower) |
| `eval.tools.enabled: true` | 12 | 12/12 | $0.0888 | 14.0 s | 1.38 | 0.99 | −0.0058 … +0.0227 | −1.9 … +1.3 | no |
| `goal.enabled: false` | 4 | 4/4 | $0.0421 | 7.6 s | 1.51 | 0.96 | −0.0024 … +0.0301 | −1.6 … +1.3 | no |
| `eval-off` | 4 | 4/4 | $0.0496 | 7.9 s | 1.96 | 0.98 | −0.0042 … +0.0590 | −1.6 … +1.7 | no |
| `advisor-every3` (pre-J2) | 4 | 4/4 | $0.0310 + $0.022 advisor | 9.6 s | 0.99 (main only) | 1.30 | −0.0026 … +0.0010 | +0.4 … +3.5 | no (slower) |
| `lsp.diagnosticsOnEdit: true` (the shipped value; `lsp-diag-off` is the base here) | 4 | 4/4 | $0.0319 | 9.3 s | 1.01 | 1.19 | −0.0004 … +0.0013 | +0.8 … +2.4 | no (on is slower) |
| `cachewarm-idle` | 4 | 4/4 | $0.0328 | 8.4 s | 1.04 | 1.05 | +0.0003 … +0.0023 | −0.2 … +1.0 | no (costs more) |
| `tools.commandDocs: builtins` | 4 | 4/4 | $0.0336 | 8.3 s | 1.06 | 1.07 | +0.0010 … +0.0038 | +0.3 … +0.8 | no |
| `vault.enabled: true` | 4 | 4/4 | $0.0408 | 8.5 s | 1.48 | 1.09 | −0.0009 … +0.0289 | −0.2 … +1.9 | no |
| `tools.commandDocs: inline` | 4 | 4/4 | $0.0479 | 8.3 s | 1.81 | 1.20 | −0.0007 … +0.0463 | −1.6 … +2.6 | no |
| `advisor-off` | — | — | — | — | — | — | — | — | = base (A0 pins `advisor.enabled: false`, 0619) |

- Boss overlay (step 4, the replay stand-in, no model): the boss launch's own overlays and manual over the same scripted 13-request session; 5 base runs agreed byte for byte.

| Setting | Request bytes (13 requests) | Per request | Δ per request | Tool the boss manual uses? |
|---|---|---|---|---|
| base | 495 110 | 38 085 | — | — |
| `lsp.enabled: false` | 491 560 | 37 812 | −273 (−0.7 %) | no |
| `debug.enabled: false` | 494 280 | 38 021 | −64 (−0.2 %) | no |
| `astEdit.enabled: false` | 494 190 | 38 014 | −71 (−0.2 %) | no |
| `browser.enabled: false` | 477 000 | 36 692 | −1 393 (−3.7 %) | no |
| `edit.mode: none` | 461 856 | 35 527 | −2 558 (−6.7 %) | no (the boss never writes to a project) |
| all five | 434 736 | 33 441 | −4 644 (−12.2 %, ≈1 160 tokens) | — |

- The boss manual (`config/boss-manual.md`) uses only bash (Jocasta's tools are bash commands) and reading skills; none of `lsp`, `debug`, `ast_edit`, `browser` or `edit` appear in it or its three skills.

## Options
| Option | Pros | Cons |
|---|---|---|
| Adopt what the trials proved: keep `todo.eager: preferred`, keep 0655's medium default, the boss's five tool cuts | Each change is measured; no change without a win | The boss cuts rest on the replay; the live boss check is still to run |
| Also adopt the near misses (`edit.mode: magic`, `goal.enabled: false`, `lsp.diagnosticsOnEdit: false`) | Possible time savings | No interval below zero on the 12 cases, or a quality trade the small cases can't price |

## Recommendation
Adopt what the trials proved.

## Choice
His answers of 2026-11-26 and the approved plan; numbers from the 2026-11-27 trials:
- `todo.eager: preferred` stays in `config/omp/config.yml` (cost −19 % in the screen, −16 % in confirm, equal passes); its comment names this re-check.
- Thinking at medium: cost × 0.94 and time × 0.94 at equal passes; 0655's `modelRoles.default` at medium stands, so no `defaultThinkingLevel` line.
- `config/boss.yml` gets `lsp.enabled: false`, `debug.enabled: false`, `astEdit.enabled: false`, `browser.enabled: false`, `edit.mode: none`: −12 % request bytes on the replay, none of the tools in its manual.
- Nothing else changes: `edit.mode: magic`, `goal.enabled: false`, `task.crossInstance: false`, `tools.commands: false`, `tools.commandDocs`, `vault`, `eval.tools`, binja, eval off and idle cache warming did not win; the advisor arms were 30 % slower with no cost gain on the 12 small cases, which can't price 0695's long-session finding, so J2's values stay; diagnostics on edit stay on (a quality trade). `overlay_text` is unchanged.

## Reason
Not given; the plan's T1.

## Consequences
- `config/boss.yml` carries the five lines; `config/omp/config.yml` changes only its `todo.eager` comment. `mise run omp-seams` checks the keys against omp 19.2.1.
- The seven arm files are in `evals/variants/` (they extend `a1` and load on the current harness); the boss-replay variants and `harness.patch` stay in the run folder.
- Pending: the live boss session check (start a thread, ask its status, `show` a plan, finish it) with the five `boss.yml` lines; the harness has no scripted boss session, so it waits for a live run. T1 step 5, the task benchmark for `a1` and the winner, waits for `tasks.jsonl` (on his Windows PC only).
- Re-run when omp's registry changes (the `omp-latest` workflow shows when).
