# 0324 Shake at task end and useless-result elision

- Status: decided
- Kind: product
- Phase/slice: 3/3.4
- Date: 2026-10-08

## Question
Which of omp's context methods join billion-context?

## Facts
Read 2026-10-08 in oh-my-pi `docs/compaction.md`: methods `remote`, `snapcompact`, `handoff`, `shake`, `soft`; `shake` replaces big old tool results and code blocks with `artifact://` references, with no model call, and can run when idle; useless-result elision blanks flagged results (empty searches) to `[Uneventful result elided]` when the cache is cold; tool-output pruning keeps the newest 40,000 tokens; async compaction starts a summary before the threshold.
- Anthropic's prompt cache lasts 5 minutes (0184): shrinking context mid-task breaks it; at task end or after 5 minutes idle it is cold anyway.
- Pruning and async compaction overlap billion-context's own folding; explained at his request.

## Options
| Option | Pros | Cons |
|---|---|---|
| Shake at task end or after 5 minutes idle | Next task starts lighter at no cache cost | — |
| Useless-result elision | Drops noise | — |
| Tool-output pruning | — | Fights billion-context; unrecoverable |
| Async compaction | — | Nothing to hide with billion-context |

## Recommendation
—

## Choice
Two Jarvis steps on top of billion-context: shake when a task ends or after 5 minutes idle (big old tool results and code blocks become `artifact://` references, readable back), and useless-result elision (empty or no-op results become `[Uneventful result elided]` when the cache is cold).

## Reason
"i feel like shake at the end of the task also will save a lot maybe idk?"; on the rest: "what are the overlaps on unticked?"

## Consequences
- Uses 1.8's `artifact://` router.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
