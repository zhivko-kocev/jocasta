# 0273 Dictation where, mode and submitting

- Status: decided
- Kind: product
- Phase/slice: 4/4.8
- Date: 2026-10-08

## Question
Where does dictation work, how is it triggered, and does it submit?

## Facts
- 0257's layout shows "hold space to talk". Asked as a tick-list (0089); all ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Where: the boss chat and every tab | — | — |
| Mode: hold Space when the input is empty; tap the mic button to toggle | — | — |
| Submitting: never auto-submit; text goes into the input | — | — |

## Recommendation
All three.

## Choice
Now (0357): dictation works in the boss chat only; TUI tabs have none.

Originally: Dictation works in the boss chat and every tab; hold Space when the input is empty, or tap the mic button to toggle; text goes into the input and is never auto-submitted.

## Reason
Not given.

## Consequences
- Slice 4.8 is decided.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0556: dictation is omp's own speech-to-text in the boss pane; Right Ctrl toggles it from anywhere, and the text is sent on release.
