# 0285 Plan mode

- Status: decided
- Kind: product
- Phase/slice: 6/6.1
- Date: 2026-10-08

## Question
How does plan mode work, and when does it start? (fresh agenda, 0088)

## Facts
Verified 2026-10-08 in Pi 1.1.0 (`examples/extensions/plan-mode/index.ts:1-13,141-146`): `/plan` or Ctrl+Alt+P toggles it; write tools are disabled; bash is limited to read-only commands; numbered steps are pulled from a "Plan:" section; a widget tracks progress.
- 0199 made plan mode one of three modes; 0239 keeps tools declared and denied so the prompt cache stays stable; 0238 is the read-only shell list.
- His first answer: "recomended + maybe every task starts with plan mode automatically"; a follow-up named the cost: boss-started threads wait for his approval.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's example, writes denied by the guard, 0238's list, approve to run | Small; cache-stable | — |
| Pi's example as it is | No changes | Cache break on each switch |
| opencode-style plan and build agents on Tab | Familiar | Two agents |
| Follow-up: plan first in every workspace thread, he approves every plan | Nothing changes code unseen | Unattended work waits |
| Follow-up: plan first, small plans auto-approved | Less waiting | Some plans run unseen |
| Follow-up: plan first only on request | No waiting | — |

## Recommendation
Pi's example, guard-denied.

## Choice
Pi's plan-mode example, adapted: writes are denied by the guard instead of disabling tools, bash uses 0238's list, `/plan` or Ctrl+Alt+P toggles it. Every workspace thread starts in plan mode; it writes a numbered plan and waits, shown as `blocked` with the plan in the boss's digest, until he approves it; approval switches to his previous mode and runs the steps.

## Reason
"recomended + maybe every task starts with plan mode automatically"

## Consequences
- Boss-started threads (0265) stop at their plan; 4.5's "start right away" now means start planning.
- Differs from the recommendation (plan first by default).
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
