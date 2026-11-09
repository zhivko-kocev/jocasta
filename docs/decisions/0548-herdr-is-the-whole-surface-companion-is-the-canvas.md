# 0548 herdr becomes the whole work surface: the rail moves into a herdr pane, the companion window becomes the canvas, one screen each

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V1, V4, V5, V6
- Date: 2026-11-08

## Question
He wants a lightweight companion that blends with herdr across two screens (the laptop's and an external monitor), one of them shown when only one screen is there. How do the companion and herdr divide the work?

## Facts
Verified 2026-11-08.
- The companion and herdr share one palette (0401, `src/frontier/ui/palette.css`) and herdr's state colours (0415); the companion differs on purpose (0369): Mona Sans and Geist Mono, 26 px rows, a greeting and the Presence, and on the dashboard a third column (clock, weather, news, activity, machine, leftover worktrees; 0429, 0439).
- Measured 2026-10-24 (`docs/testing.md`): about 625 ms to ready, about 1.29 GB idle across 21 processes, 431 MB of it the boss's omp tree.
- herdr's sidebar already shows each thread's state, model and cost (0402) and its tab bar the HUD (0437); a herdr pane runs any terminal program.
- Electrobun lists displays (`Screen.getAllDisplays`) without a plug or unplug event, so the companion checks every 5 s (0428); nothing reports which display herdr's terminal is on; finding its window through Win32 from the `jocasta` process is possible [INFERENCE: not built].
- The canvas (whiteboard, V6) and the Allow, close and toast windows (0519) need a webview or a native window.

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr-like, light | Its own window on its own screen, restyled like herdr | Still a full webview running all the time |
| Keep, font and density only | Least work | Doesn't blend |
| Rail inside herdr | Everything in the terminal; the webview loads only for the canvas | The biggest rebuild; the today view becomes text; the Presence goes |

## Recommendation
herdr-like, light.

## Choice
- herdr is the whole work surface. What the rail shows (today: calendar, what needs him, landed, underway, next, spend and quota) moves into a `jocasta` text pane in herdr; herdr's sidebar keeps the threads.
- The companion window becomes the canvas: it opens when an agent draws or a review is shown. The Allow, close and toast windows and push-to-talk stay.
- Screens: with two, herdr's terminal is on one and the canvas on the other (a setting picks which, a hotkey swaps them); with one, one is in front at a time, switched by a hotkey or the tray.
- A herdr experience pass follows before the build and records its own decisions: the text pane's contents and layout; how it's drawn (a terminal UI library is a new dependency, his to pick); how a pane that isn't omp starts, restarts and is told apart from threads (0518's heal, 0521's restart, herdr's agent state and H3's checks all assume omp panes); the greeting, the Presence and the extras column (news, weather, clock, machine); and the hotkeys.

## Reason
"rail inside but let's focus on making herdr the full dev experience then see settings ui settings everything"; screens: "Two: one each; one: toggle".

## Consequences
- Differs from the recommendation.
- Amends 0369 (the composed look), 0409 (rail and canvas in one window), 0428 (the companion as the dashboard), 0429 and 0439 (the third column).
- H5 and H6 measure the new shape when the benchmarks run again (0533).
- His "then see settings ui settings everything" is a separate review of every setting and the UI, after the herdr pass, not part of this record.
- Amended by 0551: the pane is drawn with OpenTUI. Amended by 0552: no companion window at all; diagrams open in the browser, and the screens setting applies to herdr's terminal and that browser window.
- Amended by 0553: `today` sits beside the boss (65/35) as a herdr plugin pane. Amended by 0555: the screen hotkeys go.
