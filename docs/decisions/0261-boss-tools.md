# 0261 The boss's tools

- Status: decided
- Kind: product
- Phase/slice: 4/4.4
- Date: 2026-10-08

## Question
Which tools does the boss have?

## Facts
Read 2026-10-08 in firstmate (kunchenguid/firstmate, commit b062eb9, MIT): its commands spawn a crewmate in its worktree (`bin/fm-spawn.sh`), write the brief (`bin/fm-brief.sh`), send to a durable steering inbox (`bin/fm-send.sh`), read current state (`bin/fm-crew-state.sh`), interrupt or relaunch (`bin/fm-control.sh`), tear down with a refusal for unlanded work (`bin/fm-teardown.sh`), and snapshot the fleet (`bin/fm-bearings-snapshot.sh`). A scout task writes a report instead of a PR (`bin/fm-brief.sh:609-618`). Digests don't scrape conversation history (`.agents/skills/bearings/SKILL.md:55`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Six tools; `thread_start` takes a ship or scout shape | Small; firstmate's two task shapes | — |
| Six tools, ship only | Smaller | No report-only tasks |

## Recommendation
Six tools with ship and scout.

## Choice
`projects()`, `thread_start(project, brief, shape)` with `shape` `ship` (changes code on its branch) or `scout` (writes a report, no code changes), `thread_status(thread)`, `thread_send(thread, text)`, `thread_stop(thread)`, `show(thread)`. No file or shell tools.

## Reason
Not given.

## Consequences
- Appendix A's task-shapes row (0217) is v1 through this record.
- Amended by 0372: the boss also draws.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
