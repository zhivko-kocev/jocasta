# 0521 An omp that ends in an open Jocasta pane is started again

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-11-02

## Question
H3's pane fault kills omp and leaves its pane open, and nothing starts omp again there. What should happen when omp ends in a Jocasta pane that stays open?

## Facts
Verified 2026-11-02 in the VM's benchmark sandbox, on 0517's build.
- H3 killed the boss pane's omp (09:49Z, pane `w1:p7`); nothing started it again in 120 s. A thread pane's omp killed by hand (09:46Z, `w14:p5`) stayed a bare shell for 60 s.
- The service relaunches omp only in panes herdr restored in a new terminal after a server restart (`LaunchedPanes.restored`, `src/frontier/server/boss.ts:137`). A pane whose omp ended in the same terminal isn't one.
- No record decides an omp that ends in an open pane; 0482's H3 assumed "the service respawns it".
- H3's companion fault recovered in 2.1 s with the same panes.

## Options
| Option | Pros | Cons |
|---|---|---|
| Start it again | A crash heals like herdr's (0518); H3's pane fault as designed | An omp he quits on purpose (`/exit`) in a Jocasta pane comes back too; goes into the new build |
| Boss only | His own exits in thread panes stay exits | A crashed thread needs him; H3's pane fault recovers only when it hits the boss |
| Keep; H3 records it | No change | A crashed boss leaves Jocasta without its boss until he restarts; the thesis reports it |

## Recommendation
Start it again.

## Choice
The service starts omp again, resuming its session as it does for a restored pane, in the boss pane, a thread pane or a project pane that Jocasta launched, when that pane's omp ended while the pane stayed open, unless Jocasta itself ended it (a thread's finish or close, Stop everything, a stop). An omp he quits on purpose in such a pane comes back too; closing the workspace still ends a thread (0490).

## Reason
"Start it again".

## Consequences
- H3's pane fault measures this restart (0522).
- Amended by 0549: on Jocasta's start, a thread that was mid-turn when Jocasta closed continues that turn (`omp --restore`); the others reopen idle.
