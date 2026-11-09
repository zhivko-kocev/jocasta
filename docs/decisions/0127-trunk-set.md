# 0127 The trunk set

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
Which branches are trunk?

## Facts
Counted 2026-10-07, read-only, over the 37 repos under `D:/`:
- `origin/HEAD` points to `main` in 23, `master` in 7, `development` in 2, and is unset in 5.
- Local branches present: `main` 24, `develop` 11, `master` 10, `development` 3, `dev` 0.
- What changed (0004): `development`, missing from v4's list, is a trunk in his repos.

## Options
| Option | Pros | Cons |
|---|---|---|
| The `origin/HEAD` target plus `main`, `master`, `develop`, `development`, `dev` | Covers every trunk in his repos | A branch named `development` can't be worked on |
| v4's list without `development` | As planned | Three repos' trunk gets full tools when `origin/HEAD` is unset |
| `origin/HEAD` only | One source | Five repos have no trunk |

## Recommendation
Add `development`. This changes the plan's first option because of the count above.

## Choice
Trunk is the `origin/HEAD` target plus `main`, `master`, `develop`, `development` and `dev`.

## Reason
Not given.

## Consequences
- The trunk rule (0117) applies to these branches.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
