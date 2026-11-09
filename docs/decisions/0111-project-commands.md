# 0111 `project` commands

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
Which `project` commands does Jarvis have?

## Facts
- Appendix I claimed `project` `list`, `rename`, `add`, `remove` and `rescan`. Pi has no `project` command (section 7). How commands reach the CLI is 1.6.

## Options
| Option | Pros | Cons |
|---|---|---|
| Bare `project` (show), `list`, `add`, `remove`, `rename`, `rescan` | Every write has a command | Six verbs |
| `list`, `add`, `remove` only | Three verbs | Rename is a YAML edit; no rescan |

## Recommendation
The six verbs.

## Choice
`project list`, `project add <path>`, `project remove <name|id>`. `remove` drops the project from the catalog and adds its path to `exclude`; files are never touched.

## Reason
Not given.

## Consequences
- Appendix I's claimed commands for phase 1 drop `project rename`, `project rescan` and a bare `project`.
- Renames are hand edits in `projects.yaml` (0110).
- Differs from the recommendation, which had six verbs.
