# 0523 H3's session check accepts a session entry anywhere in the file

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
H3's lost-work check "each pane's session file still parses" requires the file's first line to contain "session". omp 18.13.1 writes a `title` entry first. Fix the check?

## Facts
Verified 2026-11-02 in the VM's benchmark sandbox.
- All 8 thread sessions checked are JSON on every line, yet each fails `sessionParses` (`bench/h3-recovery.ts`): their first line is `{"type":"title",…}`.
- So all 3 H3 rows so far said "sessions parse: false" without any damage.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fix the check | The check tests what it says; no false lost work | A harness change before the run |
| Keep it | No change | Every H3 row reports damaged sessions that aren't |

## Recommendation
Fix the check.

## Choice
A session file parses when every non-empty line is JSON and a `session` entry is in it, wherever it is.

## Reason
"Fix the check".

## Consequences
- Amends 0482's H3 before the run, as 0482 asks of any change.
