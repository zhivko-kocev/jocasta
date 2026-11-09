# 0177 Tool-call format

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
Which tool-call format does Jarvis use?

## Facts
Verified 2026-10-07: Pi 1.0.1 uses provider-native structured tool calls only; its core has no text dialects.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Provider-native only | Reliable tool calling | — |
| Allow text dialects | — | Degrades tool calling |

## Recommendation
Provider-native only.

## Choice
Provider-native structured tool calls only.

## Reason
Not given.

## Consequences
- Appendix G's lever is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
