# 0264 The boss's model

- Status: decided
- Kind: product
- Phase/slice: 4/4.4
- Date: 2026-10-08

## Question
Which model and thinking level does the boss use?

## Facts
- His default is `claude-opus-5-5` with thinking `high` (0056). Pi 1.1.0 added `claude-haiku-5-5` (Pi changelog 1.1.0). The boss routes, relays and summarizes, and never sees thread content (0262).

## Options
| Option | Pros | Cons |
|---|---|---|
| The default model with thinking `low` until phase 6's model roles | One sign-in; fast chat | Less depth |
| Haiku 5.5 | Cheaper, faster | Weaker judgment |
| The default model with thinking `high` | Same as threads | Slower chat |

## Recommendation
Default model, thinking low.

## Choice
The default model with thinking `low` until phase 6's model roles; he may raise it after using it.

## Reason
"the recomended, maybe i will change the effort after using it if i see bad performance"

## Consequences
- Slice 4.4 is decided.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
