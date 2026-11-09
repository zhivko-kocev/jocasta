# 0492 The start box follows the last project pane he focused

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
0484 asks for his Allow before a `thread_start` in a project other than the one of the last pane he focused. Which panes count as "the last pane he focused"?

## Facts
Verified 2026-10-24 in the campaign's beta day (0481), on a sandbox build.
- The box came up for every first start of the day, 4 times: herdr opens each session with a shell workspace at his home folder focused, and the boss pane is in no project, so after he talks to the boss every start counts as another project.
- The box exists for a prompt-injected boss starting work he didn't ask for (0484).

## Options
| Option | Pros | Cons |
|---|---|---|
| Remember the last project pane | A start in the project he last looked at needs no box; any other project still asks | Before he has looked at any project pane, every start asks |
| Keep it as today | No change | A box for nearly every start |

## Recommendation
Remember the last project pane.

## Choice
Jocasta remembers the last focused pane that belongs to a project; focusing a pane in no project (the boss, herdr's home shell) doesn't change it. A `thread_start` in that project needs no Allow; a start in any other project, or before he has focused any project pane in this run, asks as 0484 says.

## Reason
"Remember the last project pane".

## Consequences
- Amends 0484 (which focused pane the cross-project check compares with).
