# 0464 Session files stay the source of thread events

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
The watcher learns turn ends and compactions by parsing omp's session files, whose format keeps changing. Does Jarvis take them from omp directly through a small omp extension instead?

## Facts
Verified 2026-10-23.
- The watcher parses session JSONL, where omp's format keeps changing (the 18.11.0 seam; in 18.12.6 `model_usage`, `memory_recall`, `message.retryRecovery`, and a session moved to a sibling file with `parentSession` when a second process writes it).
- omp extensions get `turn_end`, `agent_end` and compaction events (`docs/extensions.md`, "Event surface"); Jarvis panes start with `--continue`, so extensions load before the session starts (`src/main.ts:2386-2387`).
- 0386 and 0391 keep Jarvis code outside omp; he kept that rule for the guard (0450, no extension guard).

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep reading session files | No Jarvis code inside omp (0386, 0391 hold) | Each format change needs the reader updated |
| One small extension posting events; parsing kept to catch up | Typed events; less format chasing | Jarvis code inside omp; re-checked on every bump; can break sessions if it misbehaves |

## Recommendation
Keep reading session files.

## Choice
The watcher keeps reading omp's session files; no Jarvis code goes inside omp (0386, 0391 hold). The reader learns 18.12.6's entry kinds: `model_usage` entries count toward spend (0452); turns marked `retryRecovery` are skipped when picking a thread's last line, question and test result; `memory_recall` entries are ignored; a session moved to a sibling file is followed through its `parentSession`. Re-checked on every omp bump (0383).

## Reason
"Keep reading session files (no Jarvis code in omp)".

## Consequences
- 0386 and 0391 are unchanged.
- Plan appendix I's watcher paragraph names the entry kinds it reads.
