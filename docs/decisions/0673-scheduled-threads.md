# 0673 Scheduled threads

- Status: v1
- Kind: product
- Phase/slice: HUD and orchestration round, after the fork build
- Date: 2026-11-23

## Question
He ticked "Scheduled threads: a brief that runs on a schedule ('Mondays: dependency bumps'), like the weekly review" (omp #2763). How are they written and started?

## Facts
Verified 2026-11-23: the weekly review (0470, 0583) already starts a headless run on a weekday; `thread_start` starts a thread with a brief, `done_when` and `heavy` (0540, 0590) in plan mode, so a started thread waits for his plan approval.

## Options
- **A `schedules.yml` per project with weekdays and a time.** Pros: readable. Cons: no cron.
- **A cron expression.** Pros: any cadence. Cons: a new crate; harder to read.
- **Through the boss's tool only.**

## Recommendation
`schedules.yml` with weekdays and a time.

## Choice
His answer of 2026-11-23: each project's Jocasta folder may hold a `schedules.yml`: a list of schedules, each with a brief, days (`mon`…`sun`), a time (`HH:MM`, local), and optional `done_when` and `heavy`. At its time the service starts the thread as the boss's `thread_start` does, plan first, so it waits for his approval; a run missed while Jocasta was closed starts once at the next start. Upcoming runs show in the dock's NEXT.

## Reason
Routine work starts by itself; his attention goes only to the plan.

## Consequences
- A new file format in the project's data folder; the manual documents it.
- Each run costs a thread's tokens; nothing runs without his plan approval.
