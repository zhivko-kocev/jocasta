# 0601 The completion probe is off in Jocasta's panes

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-14

## Question
omp's completion probe costs $8.23 of side spend and 22.4M prompt tokens in the window. What does it give Jocasta, and does Jocasta keep it?

## Facts
- `task.completionProbe` (default true) sends a cached side request per running subagent at +2, +5, +10, +30 minutes, then hourly, for subagents of an interactive main session (omp 18.20.0 `src/task/completion-probe.ts:1-20,38-42`; `docs/research/state-of-the-art.md` section 2).
- Its only output is an estimated percent and a label in omp's `wait` and `task` views. Jocasta's service does not read it: no `completionPercent` or `AgentProgress` reader in `src/` ([INFERENCE] from absence).
- Finished-task detection does not use the probe: the thread's `report_result` and Jocasta's idle check do that.
- The probe's cache reads keep a 5-minute cache warm; his subscription writes 1-hour cache entries (0586), so that side effect is not needed.

## Options
| Option | Pros | Cons |
|---|---|---|
| Off in Jocasta's overlay for panes | Removes the side spend and the prompt load; one config line | No progress percent in omp's own wait views |
| On | omp's views unchanged | About $8 a week continues |

## Recommendation
Off in the overlay (option 1). It is decoration in every view Jocasta reads.

## Choice
`task.completionProbe: false` in Jocasta's pane overlay (`overlay.ts`). His own `config.yml` is not changed by this record; his plain omp sessions keep the probe.

## Reason
His answer: "off", after the question "if we turn it off what will happen" was answered with the effect above.

## Consequences
- The matching test text in the overlay is updated in the same build.
- The baseline's probe line stays as the before measurement; the after measurement uses the same window rule.
