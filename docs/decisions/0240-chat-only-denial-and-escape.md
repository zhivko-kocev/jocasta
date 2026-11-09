# 0240 Chat-only denial text and escape hatch

- Status: decided
- Kind: product
- Phase/slice: 2/2.7
- Date: 2026-10-08

## Question
What does a chat-only denial say, and is there an escape hatch?

## Facts
- `/thread new` is the TUI command for a workspace thread (0123, 0132, 0133).

## Options
| Option | Pros | Cons |
|---|---|---|
| The previewed message, no escape hatch | The rule is the point; a workspace thread is one command away | — |
| The message plus a logged one-time `--allow` | Flexible | A bypass to build and audit |

## Recommendation
The preview, no escape hatch.

## Choice
Now (v1 beta, 2026-10-09, under 0363): `chat-only thread: edits happen in a workspace thread (start one with /thread new "<title>" --brief "<brief>")`, because 0123 and 0315 make the brief required and the old hint led a model to the usage error. No escape hatch.

Originally: `chat-only thread: edits happen in a workspace thread (start one with /thread new "<title>")`; no escape hatch.

## Reason
Not given.

## Consequences
- None.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
