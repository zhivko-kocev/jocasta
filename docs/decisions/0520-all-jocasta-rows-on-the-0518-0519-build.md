# 0520 Every Jocasta row runs again on the build with 0518 and 0519

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
0518 and 0519 change the build the benchmarks measure. Which benchmark rows run on the new build?

## Facts
Verified 2026-11-02.
- 0517 put every Jocasta row on one build (`jocasta-windows-x64.zip` `dac25d11…`). On it ran H2 with the window in front and minimized, H5, H6 and H8 (2026-11-01/02); H3 stopped after its first run (0518).
- 0518 and 0519 touch closing, the close question, the Allow box and the toasts. None of those runs closed Jocasta, but H2 measures the toasts, and the operator presses the Allow box in H2, H6 and H8 (0516).
- H2's t3 is read from Windows' notification database (`bench/h2-toast.ts`, `readNotifications`), which lists no window of an app's own.

## Options
| Option | Pros | Cons |
|---|---|---|
| Only H3 | The full run starts about 3 h after the answer | Two builds in the results; the thesis names both |
| All again | One build for every Jocasta row, as 0517 chose | About 6 more hours before the full run |

## Recommendation
Only H3.

## Choice
All again: on the build with 0518 and 0519, H2 (window in front, minimized, and behind the terminal, the pass that measures t3), H5, H6, H8 and H3 run again, then the full run. 0517's rows stay in the kit as a comparison, in folders named `-0517`, as the release's are named `-release`.

## Reason
"All again".

## Consequences
- Differs from the recommendation.
- H2's t3 becomes the moment Jocasta's toast window opens, read by a UI Automation watcher in the bench instead of Windows' notification database (amends 0482's H2).
- The operator (0516) presses Allow in Jocasta's Allow window instead of the system box.
- Amends 0517.
