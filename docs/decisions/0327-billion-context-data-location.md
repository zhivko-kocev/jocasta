# 0327 billion-context's files under the data dir

- Status: decided
- Kind: product
- Phase/slice: 3/3.4
- Date: 2026-10-08

## Question
Where does billion-context keep its config, sessions and logs?

## Facts
- By default it writes `%USERPROFILE%\.config\billion-context`, `.local\share\billion-context` (sessions and a root CA key for its launcher mode, unused by Jarvis) and `.local\state\billion-context` (logs), overridable through `XDG_*` variables. Jarvis's test runs created them on his machine on 2026-10-08.

## Options
| Option | Pros | Cons |
|---|---|---|
| Under `<data>/billion-context`; delete the test-created folders | `JARVIS_DATA_DIR` isolates it | — |
| billion-context's defaults | — | Files outside Jarvis's dirs |

## Recommendation
Under the data dir.

## Choice
Jarvis points billion-context's config, data, state and cache at `<data>/billion-context/{config,share,state,cache}` through the proxy child's environment only, so `JARVIS_DATA_DIR` isolates it. The folders created by the test runs were deleted.

## Reason
Not given.

## Consequences
- Appendix I: the data dir layout gains `billion-context/`.
- The cache dir (it holds `models-dev.json`) was found during the build and added under the same rule.
