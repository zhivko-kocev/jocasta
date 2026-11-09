# 0477 mise for every project, kept outside the repo

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
He wants Jarvis and its panes to use mise's features (tools, env, `.env` loading, tasks) on every project to speed up work, without committing anything mise-specific to the project. Where does a project's mise config live, and how do threads get it?

## Facts
Verified 2026-10-23.
- mise reads global config from `~/.config/mise` (`MISE_CONFIG_DIR`); environment files `config.<env>.override.toml` there load when `<env>` is in `MISE_ENV`, and "the override file is global config, so it is trusted by path" (https://mise.jdx.dev/configuration.html).
- Project configs (`mise.toml`, `mise.local.toml`) need trust, and omp's `CI=true` would make mise trust them by itself (0473); a new thread worktree has none of the project's gitignored files (0457).
- Jarvis passes per-pane env through herdr's `layout.apply` (`src/frontier/server/herdr.ts:524-527`).
- `.git/info/exclude` ignores a file locally in every worktree of a repo without a commit (git docs, gitignore).

## Options
| Option | Pros | Cons |
|---|---|---|
| Jarvis keeps each project's mise config in mise's global folder, picked per pane by `MISE_ENV` | Nothing in the repo; trusted by path; every worktree gets it with no copy | Lives in his mise folder; ranks above a repo's own mise config |
| `mise.local.toml` in the project, excluded through `.git/info/exclude`, copied into worktrees | Standard mise file next to the code | Needs trust in each worktree; can be edited by a thread |
| A mise list of personal projects only | Client repos never touched | Not what he asked: every project, nothing committed |

## Recommendation
Jarvis keeps each project's mise config in mise's global folder, picked per pane by `MISE_ENV`.

## Choice
- Every project can have a Jarvis-kept mise config on his machine, never committed: `~/.config/mise/config.jarvis-<project id>.override.toml` (the builder may use a lower-precedence global environment file instead if mise loads one, and lists it as built). Every project and thread pane of that project gets `MISE_ENV=jarvis-<project id>` (appended to any `MISE_ENV` the pane already has), so every worktree has the project's tools, env (including the per-thread variables of 0457 and `.env` files through `_.file`) and tasks without a copy or a trust prompt.
- When a project has no such config, the boss offers a setup thread: it finds the project's tools and test command, writes the config with `test` and `verify` tasks, and he approves its plan like any thread. The review's own check (0453) uses that `verify` task.
- A repo's own committed mise config is used only when trusted (0473).

## Reason
"we can use mise for envs and stuff yes? we can use the features on every project without commiting mise specifis stuff" and "The boss offers a setup thread that writes it".

## Consequences
- Amends 0457 (per-thread variables reach mise's `[env]`) and 0473 (Jarvis's own configs need no repo trust).
- Needs mise installed (0476); without it nothing changes.
- Plan appendix I names the file pattern and `MISE_ENV`.
