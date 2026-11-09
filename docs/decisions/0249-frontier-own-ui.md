# 0249 Frontier is Jarvis's own UI

- Status: decided
- Kind: product
- Phase/slice: 4/opening
- Date: 2026-10-08

## Question
What is Frontier's shape, now that herdr runs natively on Windows?

## Facts
Verified 2026-10-08 (https://herdr.dev/docs/windows-beta/, https://herdr.dev/docs/concepts/, https://github.com/herdrdev/herdr): herdr 0.9.3, Apache-2.0, has generally available native Windows support; a background server keeps panes running after the client closes; panes are marked blocked, working, done or idle; a CLI and socket API let agents spawn and prompt panes; Pi and omp integrations install on Windows; plugins are preview on Windows; direct terminal attach is unsupported on Windows. Plan 4.2 called its Windows support beta.
- devdotfast/whiteboard (https://github.com/devdotfast/whiteboard, MIT): a desktop app (vendored Code OSS) where agents draw sequence and ER diagrams on a canvas through an SDK, linked to the code; Windows builds exist.
- firstmate is a phase 4 design reference (0217). V1, V5 ("tabs may work like herdr's") and V6 (visuals like whiteboard).
- He first answered "herdr + jarvis but can the ui be like i imagined or?"; a follow-up showed herdr's UI with a boss pane.

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr plus a Jarvis boss pane | Least code | herdr's UI, not his |
| Spike first | Data | Delay |
| Jarvis's own Frontier UI | His UI | Most code |
| Follow-up: herdr with the HUD in the boss pane | — | — |
| Follow-up: try herdr first | — | — |
| Follow-up: own Frontier UI after all | — | — |

## Recommendation
herdr plus a Jarvis boss pane.

## Choice
Frontier is Jarvis's own UI: "like jarvis in the movie but with animatios and stuff diagrams etc everything goes through jarvis", drawing on whiteboard (agent-drawn diagrams), herdr (persistent agent panes with states) and firstmate (one liaison running a crew), and kept "simple but cool and smooth and optimal".

## Reason
"i would love my own ui like i want to be like jarvis in the movie but with animatios and stuff diagrams etc everything goes through jarvis. you can see whiteboard project for ide coding agent with diagrams and drawing and animations + herdr + firstmate and we can create something good useful simple but cool and smooth and optimal"

## Consequences
- Differs from the recommendation.
- Its surface (terminal, desktop or browser) and what runs the threads are the next questions.
- Phase 7's visuals (V6) move toward Frontier's core; whiteboard joins the catalog as a reference.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
