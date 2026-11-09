# 0052 --version output

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
What does `jarvis --version` print?

## Facts
Pi prints `VERSION` from the package.json next to the exe, which is Jarvis's version; Pi's and Bun's versions need Jarvis's entry to answer first.

## Options
| Option | Pros | Cons |
|---|---|---|
| `jarvis 0.1.0 (pi 1.0.1, bun 1.4.2)` | Bug reports carry all three pins | The entry handles `--version` and `-v` before Pi |
| Pi's output | No handling | Pi and Bun versions invisible |

## Recommendation
The three-version line.

## Choice
`jarvis <version> (pi <pi version>, bun <bun version>)`, for example `jarvis 0.1.0 (pi 1.0.1, bun 1.4.2)`.

## Reason
Not given.

## Consequences
`src/entry.ts` answers `--version` and `-v` before calling `main()`.

- Amended by 0386: the Pi version is gone with Pi; the line is `jarvis <version> (omp <version>, herdr <version>, bun <version>)`.
