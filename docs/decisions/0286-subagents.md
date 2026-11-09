# 0286 Subagents

- Status: decided
- Kind: product
- Phase/slice: 6/6.4
- Date: 2026-10-08

## Question
How are subagents built?

## Facts
Verified 2026-10-08 in Pi 1.1.0 (`examples/extensions/subagent/index.ts:1-13,300-349`): the example spawns a separate `pi` process per subagent (`--mode json -p --no-session`), with single, parallel and chain calls, and ships `scout.md`, `planner.md`, `reviewer.md` and `worker.md`; `examples/extensions/structured-output.ts` exists.
- His omp agents: default 258, reviewer 138, scout 119, task 67 (appendix A). Appendix G: concurrency 8. Agent definitions come from his config dir and bundled agents (0231).
- He asked what the cap does; it was explained.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's example spawning `jarvis`, with output schemas; bundled scout, reviewer, worker; cap 8 | Small; isolated contexts | Child processes |
| In-process sessions | No processes | Shares the parent's process |

## Recommendation
As recommended, cap 8.

## Choice
Pi's subagent example adapted to spawn `jarvis`, with an output schema per call (Pi's structured-output example). Bundled agents: scout (`jarvis/cheap`), reviewer (`jarvis/strong`), worker (`jarvis/default`); his own in `~/.jarvis/agent/agents/*.md`. At most 8 run at once per thread; more wait for a free slot.

## Reason
Not given.

## Consequences
- The eval climb loop (0168) builds on this.
- Slice 6.4 is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
