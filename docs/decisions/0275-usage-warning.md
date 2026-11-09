# 0275 Usage warning

- Status: decided
- Kind: product
- Phase/slice: 4/4.12
- Date: 2026-10-08

## Question
When does Frontier warn about the day's usage?

## Facts
- Usage left shows only when the provider reports it (0274). Toasts exist from 4.9 (0271).

## Options
| Option | Pros | Cons |
|---|---|---|
| One warning at 80% | Quiet | — |
| 50%, 80% and 95% | More notice | Noise |
| None | — | Surprise limits |

## Recommendation
One at 80%.

## Choice
One warning at 80% of the day's usage, as a toast and an amber HUD figure, when usage is reported.

## Reason
Not given.

## Consequences
- Slice 4.12 is decided.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
