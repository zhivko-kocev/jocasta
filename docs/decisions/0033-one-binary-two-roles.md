# 0033 One binary with two roles

- Status: decided
- Kind: product
- Phase/slice: 0/0.1
- Vision: V7
- Date: 2026-10-04

## Question
One `jarvis` binary with Frontier and harness roles, or two binaries (plan 5.3)?

## Facts
- Name check for `jarvis` (0024), 2026-10-04: not on his PATH (`where.exe jarvis`); no winget package (`winget search jarvis`); `zhivko-kocev/jarvis` does not exist on GitHub; npm `jarvis` is taken by an inactive package (0.0.1, created 2011, last modified 2022). npm matters only if Jarvis is published there; no collision opens a grilling session.
- Pi's binary is one `bun build --compile` exe plus two workers (plan 5.4).

## Options
| Option | Pros | Cons |
|---|---|---|
| One `jarvis` binary | One install and one version; tabs always match Frontier | Larger exe; roles release together |
| Two binaries | Smaller exes; independent releases | Version skew between Frontier and tabs; two installs |

## Recommendation
One binary.

## Choice
One `jarvis` binary with two roles.

## Reason
Not given.

## Consequences
`src/entry.ts` dispatches on the role; the Frontier role arrives in phase 4.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
