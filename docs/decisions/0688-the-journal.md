# 0688 The journal: one append-only file of events

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
Where do the weekly numbers come from?

## Facts
- The ledger holds cost; gates, sent-back rounds and sleeps are not recorded anywhere; `production.rs` computes 0621's weekly numbers.

## Options
| Option | Pros | Cons |
|---|---|---|
| Derive the numbers from logs and thread files | No new file | Not all events survive; fragile parsing |
| One append-only journal of orchestrator events | One source; cheap to write and read | A new file |

## Recommendation
The journal.

## Choice
His answer of 2026-11-24: `src/frontier/server/journal.rs`, `Journal::append(kind, fields)` writes one JSON line to `<data>/journal.jsonl` (`{"at": <ms>, "kind", "project", "thread"?, …}`); a write failure is logged and never fails the caller. Kinds: `thread_started {heavy, budget}`, `gate {depth, command, exit, ms, tree}`, `stuck {gate}`, `over_budget {spent, budget}`, `report {outcome}`, `review_ready {notes, blocking}`, `sent_back`, `allow {what, answer}`, `finished {mode, cost_usd, wall_ms}`, `published {platform, what}`, `slept`, `woke`, `stop_all {panes}`, `queued {after}`, `started_from_queue`. No prompt text and no private text. The weekly brief's numbers come from a new `weekly_numbers(lines, from_ms, to_ms)` in `production.rs`: threads finished, gate-fail rate, sent-back rate, `stuck` and `over_budget` counts, median and total `cost_usd`, median `wall_ms`.

## Reason
The weekly review reads facts, not memory.

## Consequences
Call sites: `run_checks`, the breaker, `report_result`, `ReviewFlow::ready`, `thread_send`, the Allow answer, `Finisher::finish`, sleep and wake, publish, the queue.
