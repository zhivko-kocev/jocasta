# 0310 Approval prompt kept

- Status: decided
- Kind: product
- Phase/slice: 2/2.4 (0229)
- Date: 2026-10-08

## Question
Is the approval prompt worth having, or are threads allowed everything?

## Facts
- In `yolo` (0200) unmatched calls run. What asks today: plain `git push` (0210), `branch -D` and `push --delete` (0210), `--force-with-lease` (0218), elevation (0214), and recursive deletes of files git doesn't ignore inside the project (0213). Deny rules hold in every mode. Every workspace thread starts with a plan he approves (0285); the project thread is chat-only (0117). Asks don't apply to his own `!` commands (0202).
- Asks stop a boss-run thread until he answers; the overlay and its boss relay are code to build.

## Options
| Option | Pros | Cons |
|---|---|---|
| No prompt: pushing the thread's own branch allowed; other asks block the agent and he runs them with `!` | Nothing waits on approvals; no overlay | He runs those commands himself |
| Keep the prompt as decided | He approves each risky command in place | Boss-run threads wait at pushes and deletes |
| Allow everything except denies | Fastest | Untracked deletes and elevation run unasked |

## Recommendation
No prompt: asks become "only you".

## Choice
The approval prompt stays as decided in 0224 and 0307: yes once, yes for this run, no, no with a note; the boss relays it (0269).

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
