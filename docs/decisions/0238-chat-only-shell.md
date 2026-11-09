# 0238 Chat-only shell commands

- Status: decided
- Kind: product
- Phase/slice: 2/2.7
- Date: 2026-10-08

## Question
Which shell commands run in a chat-only thread?

## Facts
- Pi's `read`, `grep`, `find` and `ls` cover reading files (0237). 2.1's parser splits compound commands (0203).
- His first answer was free text: "the recommended and broader 1 and 3" (the read-only git list and the broader list). A follow-up listed the broader list exactly; he ticked all three groups. Project checks can write build or cache files.

## Options
| Option | Pros | Cons |
|---|---|---|
| Read-only git only | Small | — |
| No shell | Smallest | No git history in trunk chats |
| A broader read-only list with project checks | Tests and lint from the project thread | Checks can write build and cache files |

## Recommendation
Read-only git only.

## Choice
Allowed, and nothing else; a compound part outside the list denies the whole command:
- read-only git: `git status`, `log`, `diff`, `show`, `branch` (listing), `blame`;
- file reading: `ls`, `cat`, `head`, `tail`, `wc`, `rg`, `Get-Content`, `Get-ChildItem`;
- project checks: `bun test`, `bun run test|lint|typecheck`, `npm` and `pnpm run test|lint|typecheck`, `cargo test|check|clippy`, `go test|vet`, `dotnet test|build`, `pytest`.

## Reason
"the recommended and broader 1 and 3"

## Consequences
- Differs from the recommendation (broader).
- Slice 2.7 is decided, and with it phase 2.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
