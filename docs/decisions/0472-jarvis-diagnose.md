# 0472 `jarvis diagnose`

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
When Jarvis misbehaves, how does he get an explanation from the evidence without anything being changed?

## Facts
Verified 2026-10-23.
- The companion writes its log to `<data>/logs/companion-YYYY-MM-DD.log` (0431).
- Omarchy's `omarchy-agent-crash` is a thin launcher; "the method lives in the diagnose-crash skill", which works from evidence and leaves the system as it found it (https://github.com/basecamp/omarchy/blob/quattro/bin/omarchy-agent-crash, line 8; https://github.com/basecamp/omarchy/blob/quattro/default/agents/skills/diagnose-crash/SKILL.md, lines 14, 72, 88).

## Options
| Option | Pros | Cons |
|---|---|---|
| `jarvis diagnose`: a read-only omp pane with the log, the service's state and a diagnose skill | Cause explained from evidence; nothing changes | One more command and skill |
| No guided diagnosis | Nothing to build | He reads the log himself |

## Recommendation
`jarvis diagnose`: a read-only omp pane with the log, the service's state and a diagnose skill.

## Choice
`jarvis diagnose` opens a read-only omp pane in workspace Jarvis (plan mode, no edits) with the recent companion log and the service's state, and a shipped diagnose skill that explains the cause from evidence and changes nothing.

## Reason
"Yes" (to a command that hands Jarvis's own log to a read-only thread that explains the cause and changes nothing).

## Consequences
- Amends 0431 (the companion log also feeds `jarvis diagnose`).
- The log stays local (P6); the skill's text is chosen by the builder and listed as built.
- Amended by 0484: the log is sent to the model provider, and the code comment and the pane's first line say so.
