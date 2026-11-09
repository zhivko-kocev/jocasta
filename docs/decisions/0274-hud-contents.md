# 0274 HUD contents

- Status: decided
- Kind: product
- Phase/slice: 4/4.12
- Date: 2026-10-08

## Question
What does the HUD strip show?

## Facts
- Pi tracks cost per session (`/session`). Whether Pi exposes the provider's usage or rate-limit headers for subscription sign-in is not verified on 2026-10-08.
- The HUD is the top strip (0257).

## Options
| Option | Pros | Cons |
|---|---|---|
| Running and blocked counts, today's spend, usage left only if the provider reports it | Small; honest | Usage may be missing |
| Threads and spend only | Smallest | No usage |
| The plan's full list | — | Load, containers, CI to build |

## Recommendation
As recommended.

## Choice
Now (0362): for subscription providers (Copilot, the Anthropic subscription), the spend shows as a list-price equivalent, labelled so.

Originally: Running and blocked counts, today's spend, and usage left only when the provider reports it (hidden otherwise). Machine load, container health and CI join only if the pruning pass keeps their phases.

## Reason
Not given.

## Consequences
- Example: `◉ 2 running · ⚠ 1 blocked · $3.10 today · 62% left`.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
