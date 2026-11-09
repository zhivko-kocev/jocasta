# 0415 State colours follow herdr

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Do the companion's state colours mean what herdr's Tokyo Night means?

## Facts
Verified 2026-10-13.
- herdr Tokyo Night (`herdr:src/app/state.rs:153-176`, `src/client/shell.rs:225-236`): working = yellow #e0af68, blocked = red #f7768e, done = teal #7dcfff, idle = green #9ece6a.
- F2's companion kept 0369's meanings (yellow = awaiting you, red = failed).

## Options
| Option | Pros | Cons |
|---|---|---|
| Companion follows herdr | One meaning everywhere | Drops 0369's champagne = awaiting you |
| Keep the difference | 0369's meanings | Same colour, two meanings |

## Recommendation
Companion follows herdr.

## Choice
In the companion: working yellow, blocked (awaiting you) red, done teal, idle green, as herdr shows them.

## Reason
"Companion follows herdr".

## Consequences
- Amends 0369 and 0401's companion colours.
