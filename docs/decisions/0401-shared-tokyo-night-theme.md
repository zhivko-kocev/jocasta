# 0401 One shared theme: Tokyo Night

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Which palette do herdr, omp and the panes use?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).
- herdr's theme is 19 UI colours of its own (`herdr:src/config/theme.rs:102-126`); colours inside panes come from Windows Terminal's scheme, which herdr passes through (`herdr:src/pane/terminal.rs:3597-3598`).
- Built into both herdr 0.9.4 and omp 18.10.3: Tokyo Night, Catppuccin, Rosé Pine, Nord, Dracula, Gruvbox, One Dark, Solarized. Windows Terminal ships none of them.
- herdr Tokyo Night (`herdr:src/app/state.rs:153-176`): panel #1a1b26, text #c0caf5, accent #7aa2f7, mauve #bb9af7, green #9ece6a, yellow #e0af68, red #f7768e, teal #7dcfff, peach #ff9e64. omp: `dark-tokyo-night`.

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr default | No work | Three looks |
| JARVIS on 16 ANSI | One JARVIS look | Three hand-made theme files |
| From scratch | A terminal-first look | A design pass |
| A theme both ship | Least work, one look | Not the JARVIS palette |

## Recommendation
JARVIS mapped onto 16 ANSI.

## Choice
Tokyo Night everywhere: herdr `[theme] name = "tokyo-night"`, omp `theme.dark: dark-tokyo-night`, a Windows Terminal scheme "Tokyo Night" with the same colours written by `jarvis setup` after his `y`, hunk on `theme = "terminal"`, and the companion page adapted to the same palette (`DESIGN.md`).

## Reason
"see if i can find herdr and omp theme that is the same and then we just adapt the other stuff, less work and most elegamt"; then "Tokyo Night"; asked whether the companion follows, "Companion on Tokyo Night too".

## Consequences
- Differs from the recommendation.
- Amends 0369's colours: the companion keeps its composed layout and fonts, on Tokyo Night colours.
- `jarvis setup` writes a Windows Terminal fragment under `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\jarvis\`, outside `~/.jarvis`, only after his `y`.
- Amended by 0458: `config/herdr.toml` sets `manifest_check = false`.
- Amended by 0474: the Tokyo Night colours come from one palette file in the repo; the other copies read or are generated from it.
