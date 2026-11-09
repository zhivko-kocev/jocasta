# 0402 Sidebar rows: state, model, cost

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
What does each herdr sidebar row show?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).
- Rows are `[ui.sidebar.agents|spaces] rows` of built-in tokens and `$tokens` a pane reports with `pane.report_metadata` (up to 16 keys per report, value ≤ 80 chars; `herdr:src/app/api_helpers.rs:205-208`); `state_labels` rename a state per pane; `ui.agent_panel_sort = "priority"` puts blocked first.

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr default rows | No work | No model or cost |
| Two lines: state, model, cost | State words, model and cost at a glance; blocked first | The companion reports metadata |
| One dense line | Compact | No state words |
| Three lines with summary | Last assistant line | Tall |

## Recommendation
Two lines.

## Choice
Agents: `[state_icon, workspace, tab]`, `[state_text, $model, $cost]`, with `state_labels` (blocked = the waiting question, done = "review ready") and `agent_panel_sort = "priority"`; Spaces: `[state_icon, workspace, $running]`, `[branch, git_status]`. The companion reports `$model`, `$cost` and `$running`.

## Reason
"Two lines: state, model, cost".

## Consequences
- Carries Superset's and cmux's ideas (0410).
