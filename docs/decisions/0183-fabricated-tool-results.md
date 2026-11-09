# 0183 Fabricated tool results

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
What happens when the model writes text that imitates a tool result?

## Facts
- Pi 1.0.1 executes only declared tool calls, so a real result can't be faked, but assistant text can imitate one; Pi has no check.
- Appendix G: the turn aborts when the model reports a result it never received. Only the markup form can be detected mechanically.

## Options
| Option | Pros | Cons |
|---|---|---|
| Abort the turn on tool-result markup in assistant text | Mechanical; catches the dangerous form | Prose claims aren't caught |
| Skip | Nothing to build | A fake result block reads like real output |

## Recommendation
Abort on result markup.

## Choice
Now (0307): after v1.

Originally: A Jarvis extension aborts the turn, naming the reason, when assistant text contains tool-result markup.

## Reason
Not given.

## Consequences
- Prose claims in workspace threads are covered by the done check (3.7).
- Amended by 0307: after v1.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
