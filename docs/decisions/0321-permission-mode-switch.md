# 0321 Permission mode switch

- Status: decided
- Kind: product
- Phase/slice: 2/2.1, 2.4
- Date: 2026-10-08

## Question
How does he switch between `yolo` and `normal`?

## Facts
- 0200 left the key and flag that switch modes to the build. `--mode` is Pi's flag and can't be used (0137). He first picked an option labelled `/mode` and `--mode-guard`, whose preview showed `/permissions` and `--permissions`; asked again with the correct names.

## Options
| Option | Pros | Cons |
|---|---|---|
| `/permissions yolo|normal` and `--permissions <yolo|normal>` | Per tab and at launch | — |
| `/guard-mode` and `--guard-mode` | — | Longer |
| No switch in v1 | Nothing to build | `normal` unreachable |

## Recommendation
`/permissions` and `--permissions`.

## Choice
`/permissions yolo|normal` switches the tab's mode; `--permissions <yolo|normal>` sets it at launch and for headless runs. `yolo` stays the default (0200).

## Reason
Not given.

## Consequences
- Appendix I gains the slash command and the flag.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
