# 0453 Proof before done

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
The review step (0425) reports the model's own "tests pass" and a flat count of notes. What counts as proof that a thread's task is done?

## Facts
Verified 2026-10-23.
- The review's result is the model's own `TESTS: pass|fail|none` line (`hunk.ts:92-93`, `sessions.ts:68,176-178`); nothing checks it.
- omp records each bash result and its exit code; a failed call is an error result with `details.exitCode` (`tools/bash.ts:758-760,802`).
- omp subagents don't see AGENTS.md (`task/structured-subagent.ts:654`); the review prompt gives the reviewer only the diff (`hunk.ts:84-86`); the approved plan's path is in the session (`sessions.ts:49`).
- The review event reads `done and reviewed: <commits> «subject», N notes, tests …` (`service.ts:1281`), with nothing about size or test edits; agents weaken or skip tests to go green (PainSingleAgents #4), and Matt Pocock reviews the tests first (`videos/claude-matt-pocock-24-apr-26.md:257,271`).
- omp's reviewer yields `priority` P0–P3 per finding (`SRC/prompts/agents/reviewer.md:35-41, 87-94`); Jarvis's prompt maps findings to `summary`/`rationale` only (`hunk.ts:88`).
- `threadFinish` checks idle, phase, a clean tree and commits ahead, not the last review (`service.ts:1811-1837`).
- Project skills load from the repo as `skill://<name>`; Boris's `/go` tests end to end before a PR (`tips/claude-boris-6-tips-16-apr-26.md:97-109`).

## Options
| Option | Pros | Cons |
|---|---|---|
| All five | Each closes a verified gap | Five changes to the review step |
| Checked tests and the reviewer's context only | Smallest | Size, test edits and blocking findings stay invisible |
| Keep the model's `TESTS:` line | No change | "tests pass" stays the model's claim |

## Recommendation
All five.

## Choice
1. A review's "tests pass" counts only when the review turn's test command actually exited 0 in the session (omp records the bash result and exit code, `tools/bash.ts:758-760,802`); a missing or failing run shows "tests fail" or "tests unverified". The model's `TESTS:` line (`sessions.ts:68`) is no longer trusted alone.
2. The reviewer gets the approved plan and the project's rules (AGENTS.md, AGENTS.local.md, lessons) in its task; omp subagents don't see AGENTS.md (`task/structured-subagent.ts:654`); today it gets only the diff (`hunk.ts:84-86`).
3. The review event and summary show the diff size (`+A/−D in N files`), which test files changed, and any added skip/only markers.
4. Findings marked P0–P1 by omp's reviewer are counted as blocking; `thread_finish` says so when the last review wasn't a pass (today `service.ts:1811-1837` never checks).
5. When the project has its own `verify` skill, the review step runs it.

## Reason
"All five".

## Consequences
- Amends 0425 (the review prompt and how its tests result is read) and 0426 (`thread_finish`'s reply).
- The reviewer's lessons come from 0459; `AGENTS.local.md` from 0449.
- The exact event and reply texts are chosen by the builder and listed as built; the boss manual follows.
- Amended by 0473: a trusted project's `mise run verify` (else `mise run test`) is the project's own check.
- Amended by 0512: after the full run, the review prompt asks for the test command unpiped, so its exit code counts.
- Amended by 0513 (after v1): the reviewer reads his brief beside the approved plan and checks the diff against the intent.
