# 0135 Opening the harness alone

- Status: decided
- Kind: product
- Phase/slice: 1/1.6
- Date: 2026-10-07

## Question
How does the harness open on its own once bare `jarvis` opens Frontier?

## Facts
- Principle 9 (0087): the harness opens on its own after phase 4; 1.6 and 4.10 decide how.
- 0034: bare `jarvis` opens the harness until phase 4, then Frontier; how tabs start the harness role was left to phase 4. 0033: one binary, two roles.

## Options
| Option | Pros | Cons |
|---|---|---|
| `jarvis harness`, used by him and by Frontier's tabs | One path for both; 0034 unchanged | More typing than bare `jarvis` after phase 4 |
| A `--solo` flag | Short | A role-changing flag among Pi's flags |
| Revisit 0034: bare `jarvis` stays the harness, `jarvis frontier` opens Frontier | The harness is the default | Frontier stops being the bare entry point (V1) |

## Recommendation
`jarvis harness`.

## Choice
`jarvis harness` opens the harness in the current folder (its thread per 0129), whether or not Frontier runs. Frontier's tabs launch the same command. Bare `jarvis` follows 0034.

## Reason
Not given.

## Consequences
- Appendix I claims `harness` in phase 1.
- 4.2 and 4.10 use `jarvis harness` to start tabs.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
