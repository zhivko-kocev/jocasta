# 0555 The `today` pane's contents: what needs him first, no greeting or Presence, the extras where they fit, no screen hotkeys

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V4, V11
- Date: 2026-11-09

## Question
What does the `today` pane (0551, 0553) show and in which order; what happens to the greeting, the Presence orb and the extras column; and do 0548's hotkeys for switching screens stay now that there is no companion window (0552)?

## Facts
Verified 2026-11-09.
- The rail shows the calendar, what needs him, landed, underway, next, spend and quota (0548); the extras column shows news (0442), weather, a clock and the machine (0429, 0443).
- The Presence orb and the greeting are the companion's (0369, 0370); a terminal pane can't draw the orb.
- 0548's swap and toggle hotkeys moved herdr's terminal and the companion window between screens; 0552 removed that window, and diagrams open in a browser tab.

## Options
| Item | Options (pros / cons) |
|---|---|
| Order | What needs him first: header (date, time, quota, spend), needs you, underway, landed today, next, a key line · Today's rail order |
| Greeting and Presence | Drop both · One greeting line |
| Extras | Clock only · Clock and machine · All four |
| Screen hotkeys | Drop them · Keep a swap key for herdr and the browser |

## Recommendation
What needs him first; drop both; clock only; drop the hotkeys.

## Choice
- `today` shows, top to bottom: a header with date, time, weather, machine (CPU and memory), quota and today's spend; NEEDS YOU; UNDERWAY (thread, state, time, cost, context); LANDED TODAY; NEXT (calendar); NEWS while rows remain; a key line.
- The greeting and the Presence orb go.
- All four extras stay where there is room: clock, weather and machine in the header, news at the bottom only when the pane has rows left.
- 0548's swap and toggle hotkeys go; push-to-talk stays (0552).

## Reason
"Needs-you first"; "Drop both"; "if there is room keep 4"; "Drop them".

## Consequences
- Differs from the recommendation on the extras.
- Amends 0369 and 0370 (Presence and the greeting), 0429, 0442 and 0443 (the extras move into `today`'s header and its last rows) and 0548 (the screen hotkeys).
