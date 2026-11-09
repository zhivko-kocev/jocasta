# 0113 Scopes status

- Status: decided
- Kind: product
- Phase/slice: 1/1.3
- Date: 2026-10-07

## Question
Are scopes in v1, and when are they built?

## Facts
- Nothing in phases 1–3 reads scopes; the boss's visibility (4.4) and memory sealing (phase 5) do.
- Principle 6 keeps client names out of the repo; scopes keep one client's facts out of another client's threads inside Jarvis.
- Principle 8: features not picked are left out.

## Options
| Option | Pros | Cons |
|---|---|---|
| v1, built in 1.3 | The tree exists before any memory | Built two phases before anything reads it |
| v1, built when 4.4 needs it | Built against its first reader; phase 1 is smaller | Projects from phases 1–3 need a one-time scope pass |
| After v1 | A big cut | Client facts can cross between clients' threads through memory |
| Dropped | Simplest | The same, for good |

## Recommendation
v1, built when 4.4 needs it.

## Choice
Now (0262): a scope is one label per project in `projects.yaml`; no tree, kinds, file or command.

Originally:
Scopes are in v1 and are built with 4.4, their first reader.

## Reason
Not given.

## Consequences
- Slice 1.3 moves to phase 4 and is built with 4.4; its agenda (0092) is grilled when the pass reaches 4.4.
- The `scope` command moves from phase 1 to phase 4 in appendix I.
- Projects registered in phases 1–3 get their scopes in a one-time pass when 4.4 lands.
- Amended by 0262.
