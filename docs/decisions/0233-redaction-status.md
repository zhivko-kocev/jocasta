# 0233 Secret redaction status

- Status: decided
- Kind: product
- Phase/slice: 2/2.6
- Date: 2026-10-08

## Question
Is secret redaction in v1?

## Facts
Verified 2026-10-08 in Pi 1.1.0 (`dist/core/extensions/types.d.ts:1076-1099`): a `tool_result` handler can replace `content`, `details` and `structuredContent`; `message_end` can replace a finished message; `user_bash` can supply the operations that run a `!` command. A throwing `tool_result` handler is skipped and the raw result kept (0078, re-checked 0192).
- Phase 2's exit smoke requires that a planted secret never reaches stored history. Lean Jarvis (0229).

## Options
| Option | Pros | Cons |
|---|---|---|
| v1, as one small extension | Meets the exit smoke | — |
| After v1 | — | The exit smoke loses its secret check |

## Recommendation
v1, small.

## Choice
v1: one extension redacts secrets before they are shown, sent or stored.

## Reason
Not given.

## Consequences
- Appendix A's secret-redaction row is v1.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
