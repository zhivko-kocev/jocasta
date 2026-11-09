# 0409 The companion: a rail and the canvas

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
How is the companion window laid out?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).

## Options
| Option | Pros | Cons |
|---|---|---|
| Rail + canvas | Presence, today and canvas together | A wide window |
| Canvas first | Most canvas | Today as a sheet |
| Presence window + canvas window | Small | Two windows |
| Tray + capsule only | Out of the way | No resting view |

## Recommendation
Rail + canvas.

## Choice
A left rail (~300 px: the Presence and voice state, the greeting, today's Needs you / Landed / Underway / Next, spend, the dictation hint) and whiteboard's canvas on the right. Closing hides it to the tray; while hidden, Right Ctrl shows a small capsule (listening → sent).

## Reason
"Rail + canvas".

## Consequences
- Amends 0370: the crew pills, boss chat and thread view leave the window (0392).
- Amended by 0416: a Calendar row joins the rail.
- Amended by 0419: the canvas's empty state replaces whiteboard's Welcome.
- Amended by 0428: the companion is the dashboard on the second screen, with a third column.
- Amended by 0434: the window's × closes Jarvis as one, asking first while threads are working or awaiting him; Keep hides it to the tray.
- Amended by 0439: the third column follows the window's mode.
- Amended by 0548: the rail moves into a herdr pane; the window keeps the canvas.
