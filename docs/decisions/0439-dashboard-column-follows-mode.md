# 0439 The dashboard's column follows the window's mode

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
The third column (0428, 0429) shows on any window from 1600 CSS px. When does it show?

## Facts
Verified 2026-10-15.
- The column is `@media (min-width: 1600px)` in `styles.css`, not "is the dashboard": a 1920 screen at 125% (1536 CSS px) or `frontier.dashboard.display: primary` on his 150% laptop shows no column, while a wide laptop window would.
- The companion already knows whether its window is the dashboard (0428's placement).

## Options
| Option | Pros | Cons |
|---|---|---|
| Follow the dashboard mode | The dashboard always has it; the laptop never | A query parameter for the mode |
| Lower the breakpoint | One CSS change | Laptop windows get it too |
| Keep 1600 px | No change | Scaled screens miss it |

## Recommendation
Follow the dashboard mode.

## Choice
The third column (clock and weather, news, today's activity, machine and services) shows whenever the window is the dashboard (0428) and at least 1200 CSS px wide; the laptop window never shows it, whatever its width (was: any window from 1600 px).

## Reason
"Follow the dashboard mode".

## Consequences
- Amends 0428 and 0429 (the column from 1600 px).
- Amended by 0548: the third column's future is decided in the herdr experience pass.
- Amended by 0557: the dashboard window and its display setting go (0552).
