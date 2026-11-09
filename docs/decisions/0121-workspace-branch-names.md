# 0121 Workspace branch names

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
How is a workspace thread's branch named?

## Facts
Counted 2026-10-07, read-only, over the local branches of the 37 repos under `D:/` (names not printed): 141 branches; no prefix 65 (including trunks), `feature/` 36, `backup/` 20, `feat/` 7, `hotfix/` 3, `fix/` 2.
- The plan's phase 1 exit smoke showed `feat/add-retry`.
- `git check-ref-format --branch` rejects names with spaces (probed with git 2.54.0).
- Principle 2: a fixed rule, not a model guess.
- What changed (0004): his own branches use `feature/`, not `feat/`.

## Options
| Option | Pros | Cons |
|---|---|---|
| `feature/<slug>`, overridable | Matches his habit; deterministic | Fixes are `feature/` unless overridden |
| A prefix from the title's first word | Fixes land as `fix/` | A keyword table to own; misfires |
| `feat/<slug>` | Conventional Commits style | He used it 7 times against 36 |

## Recommendation
`feature/<slug>`, overridable.

## Choice
`feature/` plus a slug of the title: lowercase, every run of non-alphanumerics becomes `-`, at most 40 characters. Naming a branch when the thread is created overrides it, and an existing branch is bound as is.

## Reason
Not given.

## Consequences
- Examples, as previewed: "Add retry" → `feature/add-retry`; "Fix login on iOS" → `feature/fix-login-on-ios`.
- The phase 1 exit smoke now expects `feature/add-retry`.
- The override's flag name is decided with the `thread` commands.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
