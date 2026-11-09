# 0551 The `jocasta` pane is drawn with OpenTUI and holds today, the thread keys and his answers

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V4, V11
- Date: 2026-11-09

## Question
0548 moves the rail into a `jocasta` pane in herdr. How is the pane drawn, and what does it do?

## Facts
Verified 2026-11-08.
- The pane shows data the service already has; it calls no model.
- OpenTUI (`@opentui/core` 0.8.1, MIT, github.com/anomalyco/opentui): a TypeScript terminal UI library on a native Zig core, loaded by Bun 1.3.14 or later through `bun:ffi`, with a native package per platform (`@opentui/core-win32-x64`, about 17 MB); its Bun core tests run on Windows x64; opencode uses it.
- Plain ANSI from Jocasta's own Bun code needs no dependency; omp's `pi-tui` ties the pane to omp's version.
- 0544's Finish and Stop buttons sat on the companion's thread cards, which 0548 moves into herdr.

## Options
| Option | Pros | Cons |
|---|---|---|
| Plain ANSI in Jocasta's Bun code | No dependency; our palette | Layout, input and redraw written by hand |
| OpenTUI | Layout, input, scrolling and mouse done; built for Bun | Not yet 1.0; a native binary per platform; a pinned dependency to re-check |
| omp's `pi-tui` | omp's exact look | Tied to omp's internals and version |

## Recommendation
Plain ANSI in Jocasta's Bun code.

## Choice
The `jocasta` pane is an OpenTUI program (`@opentui/core`, pinned) in the boss's workspace. It shows today (calendar, what needs him, landed, underway, next, spend and quota) and the threads, with keys to focus a thread's pane, Finish (merge or PR) and Stop (0544's buttons), and the questions he answers (0552).

## Reason
"open tui or plane what makes more sense and brings more value", then "OpenTUI".

## Consequences
- Differs from the recommendation.
- Install approved: `@opentui/core` and its Windows native package, pinned, re-checked at each bump (0383).
- How the pane starts, restarts and is kept apart from omp threads (0518, 0521, H3) is decided with the build's own records, as 0548 lists.
- Amended by 0553: the pane is the `jocasta` plugin's `today` pane beside the boss; how it starts, restarts and is kept apart from threads is settled there, and questions open in a popup. Amended by 0555: its contents.
