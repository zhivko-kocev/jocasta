# 0428 The companion is the dashboard on the second screen

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
He wants the companion on his second screen, fixed, like a dashboard: an OS inside the OS. How does the window behave?

## Facts
Verified 2026-10-14.
- His screens (2026-10-14): the primary is the laptop panel (1920×1200 at 150%); the second is an external 1920×1080 at 100%, above it, with its own taskbar. Today the companion and the capsule always open on the primary display.
- Electrobun 2.0.2 on Windows lists displays (bounds, work area, scale, primary), places windows by position, has `setFullScreen`, a borderless style and always-on-top; it has no display-change event, no always-at-bottom and no minimize lock; `win.ptr` allows direct Win32 calls.
- 0362: "only one frontier window can be opened".

## Options
| Option | Pros | Cons |
|---|---|---|
| One window, two modes | Keeps 0362; works with one screen | Two layouts |
| A second, dashboard-only window | The companion window stays | Amends 0362 |
| Dashboard only, always fullscreen | Simplest | Covers his only screen on the laptop alone |

## Recommendation
One window, two modes; fixed as a desktop layer.

## Choice
The companion window is the dashboard. With a second display connected it sits on the first non-primary display, borderless, filling that display's work area (its taskbar stays visible), not always-on-top; it opens there on start and returns there when the display comes back (checked every 5 s; `jarvis.yaml` `frontier.dashboard.display: secondary|primary` overrides). With the laptop alone it is today's window. Layout: the rail on the left, the whiteboard canvas in the middle, a third column on the right for the extras (0429).

## Reason
"companion is the dashboard sorry for my expression"; "Leave the Dell's taskbar"; "Rail + canvas + a third column".

## Consequences
- Amends 0409 (the third column) and 0362's window placement; the one-window rule stays.
- Amended by 0439: the third column shows on the dashboard window from 1200 CSS px, never on the laptop window.
- Amended by 0548: with two screens herdr's terminal is on one and the canvas on the other (a setting picks, a hotkey swaps); with one, a hotkey or the tray switches which is in front.
- Amended by 0557: `frontier.dashboard.display` goes with the app (0552).
