# 0276 Workshop prep

- Status: decided
- Kind: product
- Phase/slice: 4/4.13
- Date: 2026-10-08

## Question
What happens when a project opens in Frontier?

## Facts
- The project thread works in the main checkout on trunk (0117). His words: "when an project isn't opened a long time usually we need to pull also on every open we pull".

## Options
| Option | Pros | Cons |
|---|---|---|
| `git fetch`, a behind count and reopening the project thread | Never changes files | He pulls by hand |
| Also `git pull --ff-only` when clean | Up to date on open | Changes the checkout |
| Only on request | — | — |

## Recommendation
Fetch, behind count and reopen.

## Choice
On every open: `git fetch`; if the main checkout is on trunk with a clean working tree, `git pull --ff-only`; otherwise show how far behind it is. Then the project thread reopens. Containers, language servers and CI status join only if their phases are kept.

## Reason
"the recomended an maybe pull when clean i guess. when an project isn't opened a long time usually we need to pull also on every open we pull"

## Consequences
- Differs from the recommendation (adds the pull).
- A pull that isn't a fast-forward never runs; the header shows the divergence.
- Slice 4.13 is decided, and with it phase 4.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
