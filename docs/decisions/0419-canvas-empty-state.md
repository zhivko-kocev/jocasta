# 0419 The canvas's empty state

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
What does the canvas show before the first drawing?

## Facts
Verified 2026-10-14.
- He saw whiteboard's Welcome page at start ("Install the whiteboard command", "Connect your agents", "Take the tour", "Create your first session") and couldn't complete it.
- Inside Jarvis the steps can't complete: the bridge doesn't report the install status, the page has no Close, and it leaves only after the first real review; `jarvis setup` already connects whiteboard.

## Options
| Option | Pros | Cons |
|---|---|---|
| Jarvis's own empty state | Clear; no fork | Hides whiteboard's own page |
| Feed whiteboard the status | Keeps whiteboard's page | Depends on whiteboard internals |
| Just the Home list | Simplest | An empty list explains nothing |

## Recommendation
Jarvis's own empty state.

## Choice
Until the first review exists, the canvas shows Jarvis's empty state: "Nothing on the canvas yet." / "Ask Jarvis or a thread to draw a diagram or a review; it appears here." and an "Open the tutorial" button; whiteboard's Welcome page is not shown.

## Reason
"Jarvis's own empty state".

## Consequences
- Amends 0409's canvas side.
- Amended by 0432: the tutorial button says "Take the whiteboard tour".
