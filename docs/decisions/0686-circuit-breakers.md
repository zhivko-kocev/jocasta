# 0686 Circuit breakers: loop cap, cost cap, pressure sleep, stop-all

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
What stops a runaway thread?

## Facts
- One PR-review thread cost $70.25.
- `--max-cost` stops a whole session tree, subagents included (read from omp 19.1.1's setting text, `session/settings.ts:1773`, not yet run).

## Options
| Option | Pros | Cons |
|---|---|---|
| No caps; he watches | Nothing to build | A runaway turn costs real money |
| Loop cap, per-thread cost cap, pressure sleep and a stop-all key | Bounded loss; he decides at the cap | A thread can stop mid-work |

## Recommendation
Four breakers.

## Choice
His answer of 2026-11-24 (the numbers are his; the settings can change without a new record):
- Loop cap: 0677 (the same gate failing 3 times).
- Cost cap: setting `threads.maxCostUsd` (number, default 75, 0 = no cap); `thread_start` takes `budget` (dollars, at least 1), which overrides it; a heavy thread without `budget` gets twice the setting; `TaskRecord` stores `budget`; `after_overlay` appends `--max-cost <budget>` for a thread; the row shows `$<spent>/$<budget>`. First check: `omp -p --max-cost 0.01 --no-session …` in a scratch folder; if omp doesn't refuse, the ledger check alone is the breaker and sends `abort`. After each `agent_end`, at or over budget the thread's stuck state reads `over budget: $<spent> of $<budget>` and the question popup offers `Raise by $<setting>` (adds it to `TaskRecord.budget`, respawn with `--restore`) and `Stop the thread`. The boss and project panes get no cap.
- Pressure sleep: 0676.
- Stop-all: key `X` in the dock (`K` if `X` is bound) opens the confirm box `Stop every running turn?`; on yes the service sends `abort` to every connected pane, the boss included, and toasts `stopped <n> turns`; phases and stuck states don't change.

## Reason
A bounded worst case.

## Consequences
Journal kinds `over_budget` and `stop_all` (0688).
