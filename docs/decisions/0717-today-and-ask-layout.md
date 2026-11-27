# 0717 `today` and `ask` layout

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, M2 R8
- Date: 2026-11-27
- Amends: 0581 (the key line), 0683 (the dock's width and when it shows), 0553 (the question box's size)

## Question
The review of the dock found five problems (T1–T5): a key line of eleven keys whatever is picked; a 46-column dock that is gone in a window under 118 columns; thread rows whose title shrinks to 18 columns in a narrow dock; a picked row lit even when the pane has the keys; a question box that grows past readable width on a wide screen and a brief typed on one cut line. How do `today` and `ask` lay out?

## Facts
- `today` is drawn by `today/view.rs` (`render`, pure, tested in a ratatui buffer); the shell lays out the sidebar (28 columns from a 70-column window), the pane, the dock and the status line (`shell/app.rs` `layout`).
- The dock shows beside the boss's tab only; Ctrl+B Alt+T gives it the keys, Esc gives them back.
- A row's facts give way from the right until its title has 18 columns (0581 UX-4).
- His windows: an 80×30 foot window on a laptop, half a 1920-pixel screen (about 118 columns), full screen (about 240).

## Options
| Option | Pros | Cons |
|---|---|---|
| The review's T1–T5 as written | Each key line says what works on the picked row; the dock is there on any window; a narrow dock still names its threads | More layout cases to test |
| Only widen the dock | Small | Narrow windows still have none |

## Recommendation
T1–T5.

## Choice
His answers of 2026-11-26 and the approved plan (the review's T1–T5):
- T1: the key line shows the picked row's keys first (a thread: `Enter go · f Finish · s Stop · r Review`; a review request: `Enter open · R PR`; a red pipeline: `Enter open · F CI`; another link: `Enter open`; a leftover: `x Remove`), then `n New · b Brief · X Stop all`; the last keys drop while the line doesn't fit.
- T2: the dock is 35 % of the window's width, clamped to 46–72 columns, beside a pane of at least 44 columns. A window too narrow for that (under 118 columns with the sidebar) has no dock beside the pane; Ctrl+B Alt+T lays it over the whole pane until Esc (or another tab comes to the front). The status line's `Ctrl+B Alt+T today` hint shows only when a dock can open (the boss's tab is in front).
- T3: under 60 columns a thread takes two rows: its title (cut with `…` only at the dock's edge), then its facts under it, the last fact dropping first.
- T4: the picked row is lit (blue marker, selection colour across the dock) only while the dock has the keys; otherwise its `›` is grey and nothing is lit.
- T5: the question box is 70 % of the window, at least 44 and at most 88 columns wide, and at least half the window high (18 rows inside, or more). New thread shows the brief's last three wrapped rows, the first marked `Brief  … ` when rows above are hidden, the caret at its end.

## Reason
The dock is the boss's tab's side panel; it has to be there on his laptop's window and say what each key does to what he picked.

## Consequences
- `today/view.rs`: `TodayState.keys`, `NARROW`, the key line built after the rows; `KEYS` and `KEY_LINE` retired. `shell/app.rs`: `layout` takes `Dock::{None, Shut, Open}`; `Layout.docked`, `Layout.over`, `DOCK_PERCENT`, `DOCK_MIN`, `DOCK_MAX`, `PANE_MIN`. `shell.rs`: `QUESTION_MAX`; the dock clears its area. `ask/view.rs`: `BRIEF_ROWS`.
- Checked in ratatui buffers at 80×30 (the dock over a 54-column pane, 53 columns inside), 59 columns (58 inside), and a 200-column window (70 columns); and the box at 80×30, 200×30 and 200×60. Not checked in a live foot window on the host (no windows there).
