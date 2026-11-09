# 0404 The boss pane: a pinned first workspace

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Where does the boss pane sit?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).

## Options
| Option | Pros | Cons |
|---|---|---|
| Own workspace, full tab | One place, Alt+1 | None |
| Boss + scratch shell | A shell beside it | Narrower boss |
| Boss as a popup | No space taken | Not always visible |

## Recommendation
Own workspace, full tab.

## Choice
Workspace "Jarvis" is always first and always there: one tab, one omp pane in `<data>/home`; `jarvis` and the service recreate it if it was closed.

## Reason
"recommended , like one pinned tab that always is on you know the idea".

## Consequences

