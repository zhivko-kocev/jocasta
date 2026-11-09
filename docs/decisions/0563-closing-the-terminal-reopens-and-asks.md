# 0563 Closing the terminal while threads work reopens it and asks before stopping

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V3, V11
- Date: 2026-11-09

## Question
With no app window (0552), closing the terminal window leaves nowhere to ask 0434's and 0549's close question. What happens when he closes it while threads are working?

## Facts
Verified 2026-11-09.
- 0434 and 0549: closing Jocasta asks first when threads are working or waiting on him, with Stop everything and Cancel, and their counts.
- When the terminal window closes, herdr's client exits but herdr's server, its panes and Jocasta's service keep running until something stops them.
- 0549: mid-turn threads continue with `omp --restore` on the next start; 0553/0558: questions open in herdr's `ask` popup.

## Options
| Option | Pros | Cons |
|---|---|---|
| Reopen and ask | Keeps 0434's guard | A terminal window reappears right after he closed one |
| Stop silently | Simplest; mid-turn threads resume next start | A mid-turn answer is redone; no guard |
| Keep running | Nothing lost | Breaks 0549's "nothing runs without Jocasta" |

## Recommendation
Reopen and ask.

## Choice
- When herdr's client exits while any thread is working or waiting on him, Jocasta opens a new terminal window attached to the same herdr session and asks the close question in the `ask` popup (counts; Stop everything / Cancel). Cancel leaves everything as it was; Stop everything stops all.
- With nothing working or waiting, closing stops everything at once.
- The quit key asks the same question. A herdr server crash isn't him closing (0534's fix).

## Reason
"Reopen and ask".

## Consequences
- Carries 0434's and 0549's guard into herdr; amends 0549 (closing the terminal) and 0552 (where the close question appears).
