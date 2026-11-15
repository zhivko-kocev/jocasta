# 0634 ratatui draws `today` and the `ask` popup

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
What draws the Rust `today` pane and `ask` popup? Today it is OpenTUI (0551).

## Facts
Verified 2026-11-15. Crate counts are transitive, on Windows, on top of the base of 0631.
- **Today's drawing.** Both programs draw plain lines of coloured runs through a 200-line line model of their own (`src/frontier/tui.ts`). OpenTUI only renders those lines and reads keys and paste.
- **Sizes:** `today` is 462 lines and `ask` is 386.
- **The two candidates:**
  - crossterm 0.29 with unicode-width and unicode-segmentation adds 15 crates.
  - ratatui 0.30 on crossterm adds 49. It brings a diffing buffer, widgets and layout.
- **herdr** uses ratatui and crossterm 0.29.

## Options
| Option | For | Against |
|---|---|---|
| crossterm with today's line model ported | the same design, few crates | no widgets or layout engine |
| ratatui on crossterm | widgets ready if the panes grow | three times the crates for panes that draw lines; pre-1.0 |

## Recommendation
crossterm with the line model.

## Choice
`today` and `ask` are drawn with ratatui 0.30 on crossterm 0.29.

## Reason
His answer: `ratatui`.

## Consequences
- This record amends 0551 (`today` and `ask` are drawn with ratatui, not OpenTUI). It differs from the recommendation.
- **What carries over:** the panes' layout and colours (the Tokyo Night palette), and their keys and paste handling.
- **At the cutover,** `@opentui/core` leaves `package.json`.
