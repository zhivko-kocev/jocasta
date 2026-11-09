# 0423 Terminal-Bench after v1

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-14

## Question
Slice B2 (0375): run Terminal-Bench now?

## Facts
Verified 2026-10-14.
- Terminal-Bench 2.0 is superseded: 2.1, 3.0 and 4.0 (Aug 2026, 66 tasks, Apache-2.0) followed.
- Harbor 0.24.0 (Apache-2.0, Python, not installed) runs it in Docker (present, 29.5.2); it has agents for Pi, opencode and Claude Code, none for omp.
- The 4.0 leaderboard lists about $4.7K for a full Opus 5.5 run.

## Options
| Option | Pros | Cons |
|---|---|---|
| TB 4.0, 10 tasks × 1, on his OK | A sanity number | An install and a paid run |
| Full TB 4.0 | A real score | Hours and hundreds of dollars |
| After v1 | Nothing now | No external number yet |

## Recommendation
TB 4.0, 10 tasks × 1.

## Choice
Terminal-Bench (now 4.0) waits until after v1; Jarvis's own suite stays the measure.

## Reason
"After v1".

## Consequences
- Differs from the recommendation.
- Moves slice B2 of 0375 after v1.
