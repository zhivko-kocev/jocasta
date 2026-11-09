# 0326 Phase 3 as-built texts

- Status: decided
- Kind: product
- Phase/slice: 3/3.2–3.7
- Date: 2026-10-08

## Question
Are phase 3's as-built visible choices accepted?

## Facts
- No record set these; shown as a tick-list (0089); all ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| As built | — | — |

## Recommendation
As built.

## Choice
- `todo`: actions `list`, `add`, `init` (replaces the list with `items`), `done` (marks several `ids`), `clear`; `/todos` shows it.
- `ask` overlay: Space ticks options in multi questions, Enter confirms; the recommended option is marked `(Recommended)` and selected first.
- A headless question stops the run with exit 3; the next run prints `jarvis: the last run stopped on a question; this prompt answers it:` and its prompt is the answer.
- The report message in the project thread shows status, summary, commits (`none` if empty) and follow-ups joined with `; `; `report` exists only in workspace threads.
- Shake: the newest 16,000 tokens are never shaken; it runs only when it saves at least 4,000 tokens; code blocks of 400+ tokens qualify; shaken items use the spill notice `[output X KB, N lines spilled to artifact://n; read it with ranges]`.

## Reason
Not given.

## Consequences
- None.
