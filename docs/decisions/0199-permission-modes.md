# 0199 Permission modes

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
Which permission modes does Jarvis have?

## Facts
Verified 2026-10-08 in oh-my-pi v18.6.0: modes are `always-ask`, `write` and `yolo`; the default is `yolo` (`packages/coding-agent/src/tools/settings.ts:309-338`, `src/tools/approval.ts:235-273`). In `yolo`, deny rules, the user's own prompt policies and tool-level critical patterns still apply; everything else is allowed.
- Claude Code has `default`, `acceptEdits`, `plan`, `auto` (a classifier), `dontAsk` and `bypassPermissions` (https://code.claude.com/docs/en/permissions).
- He ran omp in `yolo`. Plan mode is slice 6.1. Principle 2 rules out a classifier mode.
- His first answer was free text; a follow-up asked what `yolo` does.

## Options
| Option | Pros | Cons |
|---|---|---|
| No modes; profiles carry rule differences | No second mechanism | No quick switch |
| Default, accept edits and yolo, cycled by a key | Familiar | A bypass mode undermines the guard |
| After v1 | — | — |
| Follow-up: `yolo` allows unmatched calls; asks and denies hold | omp's behavior; his safety rules can't be skipped | An unforeseen command runs |
| Follow-up: `yolo` also turns asks into allows | No prompts | "Ask me first" rules silently run |

## Recommendation
None was marked in the first question.

## Choice
Free text: "like omp: yolo + plan mode". Jarvis has a normal mode, `yolo` and plan mode. In `yolo`, a call no rule matches is allowed instead of asked (0198); ask and deny rules still apply. Plan mode is designed in 6.1.

## Reason
"like omp: yolo + plan mode"

## Consequences
- Mode table: normal — unmatched ask, ask rules ask, deny rules deny; `yolo` — unmatched allow, ask rules ask, deny rules deny; plan — 6.1.
- Chat-only (0117) stays a fixed mode of its threads, not a user mode.
- Appendix A's permission-modes row is decided; plan mode's row stays with 6.1.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
