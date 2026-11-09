# 0469 Four test rules for every thread

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
Should every thread follow Omarchy's test-run discipline?

## Facts
Verified 2026-10-23.
- Omarchy's `AGENTS.md` asks agents to write test output to a log and read it there, never to re-run tests to see output or to confirm a pass, to make a new test fail without the change, and to write no change-detector tests (https://github.com/basecamp/omarchy/blob/quattro/AGENTS.md, lines 153–171).
- Ponytail already asks for one small test for non-trivial logic (`config/ponytail.md:55`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Add the four rules to `config/ponytail.md` | Every thread gets them; no code | A few more lines in every prompt |
| Leave Ponytail as it is | No change | Wasted re-runs and tests that prove nothing |

## Recommendation
Add the four rules to `config/ponytail.md`.

## Choice
`config/ponytail.md` gains four rules: save test output to a file and read it there; don't re-run tests just to double-check; a new test must fail without the change; no tests that only check that code didn't change.

## Reason
"Yes" (to adding Omarchy's four test rules to what every thread is told).

## Consequences
- Amends 0427 (the Ponytail text every pane gets).
