# 0596 A thread's worktree trusts an unchanged copy of a trusted mise config

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-10

## Question
After the cutover (0595) he trusted the repo's own `mise.toml`, so that `mise run check` works. A thread's worktree holds a copy of it, and there mise says the copy isn't trusted: `bun` fails through mise's shim, and the review's checks don't run. How do thread worktrees get mise trust?

## Facts
Verified 2026-11-10 with mise 2026.10.17 on Windows, on detached throwaway worktrees and folders, each removed after.
- Jocasta's mise file sets `paranoid = ["config"]` (0476); `mise settings get paranoid` prints `["config"]`.
- In a git worktree of the trusted `D:\jocasta`, `mise trust --show` reports the worktree's `mise.toml` as untrusted, and `bun --version` through the shim fails with "Config files in …\mise.toml are not trusted."
- With `MISE_PARANOID=false` the same worktree shows as trusted: mise's own rule (a worktree's config is trusted when the main checkout's is) works only without paranoid trust. 0473's fact holds only then.
- Under paranoid trust, trust follows content: a trusted `mise.toml` becomes untrusted once it's edited.
- `MISE_TRUSTED_CONFIG_PATHS=<folder>` trusts a folder's configs and keeps them trusted after an `[env]` block is added: trust by folder ignores content.
- Every thread worktree is made in one place, `thread_start`'s `prepare` step, before its pane starts (`src/frontier/server/thread-start.ts`); a thread pane started again gets its env through the same `threadPaneEnv`.
- `config/guard.yml` denies `mise trust` in panes, and the shipped mise skill forbids agents to trust.

## Options
| Option | Pros | Cons |
|---|---|---|
| Jocasta trusts an unchanged copy | mise's own worktree rule, which paranoid turns off; a thread that edits `mise.toml` makes it untrusted again | A product change before he starts; amends 0473's "Jocasta never trusts a config itself" |
| Trust the thread folders by folder (`trusted_config_paths`) | One line in his own mise config; no Jocasta change | Trust ignores content: whatever a thread writes into its `mise.toml` (env, hooks) loads without him; each repo needs its own line |
| Turn paranoid mode off | mise's own worktree rule works everywhere | omp's `CI=true` then makes mise trust any config in any folder: the risk 0473 closed |

## Recommendation
Jocasta trusts an unchanged copy.

## Choice
- Before a thread's pane starts (a new thread, and a pane started again) and before its checks run (the review, the check on idle and the `check` tool), when mise is on PATH and its paranoid config trust is on, Jocasta runs `mise trust` on each config that `mise trust --show` lists as untrusted inside the worktree, when the same file in the main checkout is listed as trusted and both have the same bytes.
- Nothing else is ever trusted: not a changed copy, not a config outside the worktree, not a config the main checkout doesn't have trusted.
- Without paranoid trust Jocasta trusts nothing (mise's own rule applies then).
- The guard still denies `mise trust` in panes, and the mise skill still forbids agents to trust; only the service trusts.

## Reason
His answer: "Jocasta trusts an unchanged copy".

## Consequences
- Amends 0473: Jocasta trusts a thread worktree's unchanged copy of a config he trusted; 0473's worktree fact holds only without paranoid trust.
- Builder's choices, listed as built (0363):
  - `trustCopies` in `src/frontier/server/mise.ts`, called from `threadPaneEnv` in `src/frontier/server/pane-launch.ts` before mise's env is read, and from `checksFor` in `src/frontier/server/review-flow.ts`, so a copy that a merge from trunk brought in is trusted again at the thread's next check, not only when its pane starts again;
  - a path `mise trust --show` prints with `~` for the home folder (as on Linux and macOS) is read as under the `HOME` mise ran with;
  - mise runs with the pane's `MISE_ENV`, as 0477's other calls do;
  - each trusted copy is logged once in Jocasta's log (`[mise]`), and a failed `mise trust` is logged and leaves the copy untrusted;
  - the repo's `mise.toml` comment and README's Development section say it needs `mise trust` once.
- README's config section and plan appendix I say what Jocasta trusts.
