# 0222 Always grants

- Status: dropped
- Kind: product
- Phase/slice: 2/2.4
- Date: 2026-10-08

## Question
Where does an "always" grant live, what does it match, and how does it end?

## Facts
Verified 2026-10-08 (https://code.claude.com/docs/en/permissions): Claude Code saves "don't ask again" as a prefix rule (`npm test *`), one per subcommand of a compound command, in `.claude/settings.local.json` at the repository root, which also covers worktrees.
- opencode saves the longest known command prefix from an arity table (lesson L26).

## Options
| Option | Pros | Cons |
|---|---|---|
| A Claude-style prefix rule per subcommand (0208), keyed to the project id in the catalog DB, no expiry, listed and revoked by `jarvis guard grants` | Never lands in the repo; covers worktrees | Grants pile up |
| The same, expiring after 30 days unused | Self-cleaning | Re-asks |
| The exact command only | Narrow | Re-asked per argument |

## Recommendation
As recommended.

## Choice
Now (0307): dropped; no grants and no `jarvis guard grants`.

Originally: A Claude-style prefix rule per subcommand (0208), keyed to the project id (0107) in the catalog DB, with no expiry. `jarvis guard grants [<project>]` lists them; `--revoke <id>` removes one.

```
$ jarvis guard grants acme-api
g-3f1a  allow  Bash(npm test *)        2026-10-08
g-77c2  allow  Bash(git push origin *) 2026-10-08
```

## Reason
Not given.

## Consequences
- Grants are allow rules in the precedence order (0197), so they never override a deny or an ask rule; a grant only answers an ask that came from an unmatched call (0198).
- Grant ids are `g-` plus 4 hex, as previewed.
- Amended by 0307: dropped in the pruning pass.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
