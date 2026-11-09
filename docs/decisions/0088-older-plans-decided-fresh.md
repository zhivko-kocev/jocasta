# 0088 Older plans: decided fresh

- Status: decided
- Kind: process
- Phase/slice: 0/section 4
- Date: 2026-10-07

## Question
How does the grilling program handle a slice that carries over a v4, v5 or omp-plan design?

## Facts
- The plan carries v4 designs in dozens of places ("v4 2.1's decisions 1–18 carry over", "v4 3.1 carries over", …) and cites omp-plan items (B6, B11, G4, …).
- `D:/friday-harness/` and the v4 and v5 plans are not on this machine (AGENTS.md open items; checked 2026-10-07: not in `D:/friday`, `D:/goals` or `D:/agent`).
- 0066 wrote the identity fresh when the omp plan was missing.

## Options
| Option | Pros | Cons |
|---|---|---|
| Restore the files if he can, fresh otherwise | Keeps the approved v4 specs where he has them; never blocks | Fresh items cost more questions |
| Fresh only | Minimal; built from today's sources | Re-derives schemas and rule tables v4 already settled |
| Wait for the files | Exact v4 carry-over | The program stops at the first v4 slice until they're back |

## Recommendation
Restore the files if he can, fresh otherwise.

## Choice
Fresh only. The older plans are no longer consulted.

## Reason
Not given.

## Consequences
- A slice whose text says a v4, v5 or omp-plan design carries over gets a fresh agenda first: the decisions the design needs, from the plan text, the appendix A rows and appendix G levers that name the slice, the lessons `JARVIS-MAP.md` maps to it and the pinned clones (0085). Zhivko confirms the agenda, then each item is grilled.
- "v4 §…" and "omp plan §…" citations stay in the plan as pointers to what a slice covers; they are no longer sources.
- The plan header, section 4 and appendix D say so; AGENTS.md drops the open item about the missing files.
- Differs from the recommendation, which kept the files where he could restore them.
