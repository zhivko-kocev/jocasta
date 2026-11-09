# 0443 Machine and services refresh every 10 s

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
He asked that the dashboard's machine and services refresh their values. How often?

## Facts
Verified 2026-10-15.
- They refresh every 30 s today (`MACHINE_MS`): watched live on 2026-10-15, CPU and memory changed at 22 s and 52 s.
- Each refresh is a CPU sample, a memory and disk read, and three loopback checks (Graphiti, whiteboard, herdr); the page gets a message only when a value changed.

## Options
| Option | Pros | Cons |
|---|---|---|
| Every 5 s | Feels live; matches the display check | Twice the pushes of 10 s |
| Every 10 s | Still live; half the pushes of 5 s | CPU reads as the last 10 s |
| Keep 30 s | No change | Feels stale |

## Recommendation
Every 5 s.

## Choice
CPU, memory, disk and the services row refresh every 10 s.

## Reason
"Every 10 s".

## Consequences
- Amends 0429's 30 s sample.
- Differs from the recommendation (5 s).
- Amended by 0555: the machine's CPU and memory show in `today`'s header.
