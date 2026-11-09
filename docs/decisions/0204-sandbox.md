# 0204 Sandbox

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
Does the model's bash run in an OS sandbox?

## Facts
- Claude Code's sandbox runtime doesn't run on native Windows, and Pi's sandbox example refuses to start there (lesson L27, `examples/extensions/sandbox/index.ts:251-254` in the Pi clone).
- Codex sandboxes native Windows with a restricted token and a job object (0077).
- Docker Desktop 29.5.2 is installed (0071); bind-mounting `D:\` repos into WSL 2 is much slower (0070).

## Options
| Option | Pros | Cons |
|---|---|---|
| None in v1; the guard only | No slow mounts; no Windows security code | An allowed command reaches whatever his user can |
| A container per thread | Strong isolation | Slow repo I/O on Windows; toolchains in images |
| A restricted token with a job object | Native speed | Jarvis owns Windows security code |

## Recommendation
None in v1.

## Choice
None in v1: the guard (rules and git-guard) is the boundary.

## Reason
Not given.

## Consequences
- Appendix A's OS-sandbox row is after v1.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
