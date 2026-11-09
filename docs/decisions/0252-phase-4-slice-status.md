# 0252 Phase 4 slice status

- Status: decided
- Kind: product
- Phase/slice: 4/opening
- Date: 2026-10-08

## Question
Which phase 4 slices stay in v1 under lean Jarvis (0229)?

## Facts
- Phase 4's shape: Jarvis's own UI (0249), in-process Pi sessions (0250), a local web UI in an Edge app window (0251); 4.2 is decided by 0250.
- The recommendation kept 4.1, 4.3–4.9 and 4.12, and moved 4.10, 4.11 and 4.13 to after v1.

## Options
| Option | Pros | Cons |
|---|---|---|
| 4.1, 4.3–4.9, 4.12 v1; 4.10, 4.11, 4.13 after v1 | Vision core plus the movie feel | — |
| Any other set | — | — |

## Recommendation
As described in Options.

## Choice
v1: 4.1 process model, 4.3 layout, 4.4 boss, 4.5 dispatch, 4.6 blocked flow, 4.7 working by hand, 4.8 dictation, 4.9 notifications, 4.12 HUD and power level, 4.13 workshop prep. After v1: 4.10 logon autostart and its commands, 4.11 role profiles (the boss's own profile is decided in 4.4).

## Reason
Not given.

## Consequences
- Differs from the recommendation: 4.13 stays in v1.
- Appendix I's phase 4 commands `status`, `stop` and `attach` wait for 4.10; how Frontier starts and stops is asked in 4.1.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
