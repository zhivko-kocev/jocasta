# 0349 The in-flight cap's mechanics and subagent approvals

- Status: decided
- Kind: product
- Phase/slice: 6/6.2, 6.4
- Date: 2026-10-08

## Question
Is the in-flight cap's mechanism accepted, and in which permission mode do subagents run?

## Facts
Verified 2026-10-08.
- The cap (0311) is off unless he sets a number. As built, every Jarvis process records the slot it holds in `<data>/inflight/<provider>/`; a crashed process's slot is freed.
- A subagent is a separate `jarvis` process and doesn't see its tab's mode; as built it ran in `yolo` (0200).
- omp runs subagents in yolo because they are headless; the parent's `task` call is the approval point, an exec-level tool that shows each agent and task, and user approval policies still apply inside (`packages/coding-agent/src/task/executor.ts:1055-1058`, `task/index.ts:565-583`).

## Options
| Option | Pros | Cons |
|---|---|---|
| omp's way: children in yolo; in a normal tab the `subagent` call asks once, showing agents and tasks | One clear decision per spawn; unattended children | Inside a child, unmatched commands don't ask |
| Children inherit the tab's mode | Per-command asks | Many cards from subagents |
| Always yolo, no spawn approval | Fewest asks | Normal mode is bypassed |

## Recommendation
omp's way.

## Choice
- The cap's mechanism is accepted; the cap stays off by default.
- Subagents run in yolo; deny rules, git-guard and 0348's read-only limits hold inside them. In a normal tab the `subagent` call asks once, showing each task's agent and text; in yolo tabs and for read-only spawns it doesn't ask; headless, the ask denies with exit 3.

## Reason
"accept but limit is off right?"; "how do other harnesses fix this, like omp for example"; then chosen.

## Consequences
- The guard decides `subagent` like an exec-level tool in normal mode.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
