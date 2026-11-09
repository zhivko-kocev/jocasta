# 0041 Pi version pin

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
Which Pi version, and how is it pinned?

## Facts
- 2026-10-04: `@earendil-works/pi-coding-agent`, `pi-ai`, `pi-tui`, `pi-agent-core` and `pi-durable` are at 1.0.1 on npm (published 2026-10-03). The plan assumed 1.0.0.
- Pi's own repo enforces pinned dependencies (`check:pinned-deps`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Exact `1.0.1`, bumped deliberately | Latest fixes; reproducible | A day old |
| Exact `1.0.0` | The version the plan's seams were verified against | First bump comes at once |
| Caret ranges | No bump work | Unreproducible; breaks plan section 7 |

## Recommendation
Exact 1.0.1; the seams 0.2 needs were re-read at `v1.0.1`.

## Choice
Exact `1.0.1` for every `@earendil-works/pi-*` package; a bump is its own change with the phase 0 smoke run again.

## Reason
Not given.

## Consequences
Plan section 5.4's facts were verified at commit `9b3c19d` (1.0.0); each phase's fact check re-reads them at the pinned tag.
