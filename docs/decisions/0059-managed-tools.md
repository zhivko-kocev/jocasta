# 0059 fd and ripgrep downloads

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Date: 2026-10-04

## Question
What happens when `fd` or `ripgrep` is missing?

## Facts
- Pi 1.0.1 `utils/tools-manager.js`: on TUI start Pi checks `<config dir>/bin` and PATH for `fd` and `rg`; if missing, it downloads the latest GitHub release into `<config dir>/bin` without asking.
- The 0.2 smoke saw both downloads. Neither tool is on this machine's PATH.
- Only `PI_OFFLINE` stops the download, and it also disables every other startup network call.

## Options
| Option | Pros | Cons |
|---|---|---|
| Ship pinned `rg` and `fd` in each release folder | No runtime downloads; pinned | Bigger archives; dev runs still download |
| Keep Pi's auto-download | Nothing to build | Unpinned binaries fetched at runtime |
| Require them installed; never download | Package manager owns them | Needs `PI_OFFLINE` or code |

## Recommendation
Ship them in the release.

## Choice
Keep Pi's auto-download into `<config dir>/bin`.

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- The plan's "no installs without asking" rule binds agents building Jarvis, not this runtime behavior.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
