# 0432 UX extras from the review

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
The UI/UX review (2026-10-14) proposed changes beyond DESIGN.md. Which are taken?

## Facts
Verified 2026-10-14.
- "Waits on him" has 8 phrasings across HUD, rail, toast, events, speech and herdr; the greeting counts 2 while Needs you lists 4.
- Quiet text contrast 3.58:1, focus ring 2.18:1, button borders 1.44:1.
- Voice failure is a red dot with raw text in a tooltip; the canvas error is a raw `ENOENT … uv_spawn` line.
- Toasts have no dismiss and aren't announced; the canvas bar shows a disabled "Reviews" and a placeholder before any review; the tutorial is whiteboard's own tour.

## Options
| Option | Pros | Cons |
|---|---|---|
| One term and true counts | Consistent | Less voice in data |
| Contrast lift | WCAG AA | Palette change |
| Plain error states | Fixable by him | More copy |
| Toasts you can dismiss | Accessible | More UI |
| Tidier canvas bar | Less clutter | Bar hidden at first |

## Recommendation
All five.

## Choice
Data surfaces say "awaiting you" (Jarvis's voice only in the greeting and speech), and the greeting counts every Needs-you item. Quiet text reaches at least 4.5:1 and the focus ring is full blue; DESIGN.md records the new values. Voice and canvas errors show a plain reason and what to do (voice: "Voice: not downloaded", a Try again), details folded away. Toasts close with × or Escape, are announced to screen readers and survive a reconnect. The canvas bar is hidden until a review is open; the tutorial button says "Take the whiteboard tour".

## Reason
Ticked all five.

## Consequences
- Amends 0369/0401's palette values and 0419's button label.
- Amended by 0440: while disconnected only the state marks dim; text keeps full contrast.
