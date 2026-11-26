# 0704 Project-named mise file

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J9
- Date: 2026-11-26
- Amends: 0477 (`MISE_ENV` gains the repository's own name before `jocasta-<project id>`)

## Question
On a client repository he may add no file, so its tools, env and `test`/`verify` tasks live in `~/.config/mise/config.<repo folder>.toml`, which ultron's shell hook loads through `MISE_ENV` in every checkout and worktree of that repository. Jocasta's panes start with their own `MISE_ENV`. Do they load that file too?

## Facts
- Every project and thread pane, and every mise call Jocasta makes for a project (its `env --json`, its tasks, its check), gets `MISE_ENV=<what the pane had>,jocasta-<project id>` (0477), which loads `config.jocasta-<project id>.override.toml` from mise's global config folder.
- `MISE_ENV=a,b` loads `config.a.toml` and `config.b.toml` from that folder, later names winning.
- ultron's shell hook names the project after the folder holding the repository's git common dir (`git rev-parse --git-common-dir`), so a worktree gets its main checkout's name.
- mise's global config folder is `MISE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/mise`, else `~/.config/mise`, on every OS (setup's `mise_config_dir`, which writes Jocasta's override files there).

## Options
| Option | Pros | Cons |
|---|---|---|
| Add the repository's name before `jocasta-<pid>` when its file exists | Panes match his shell; nothing in the repository; Jocasta's own file still wins | One git call per mise call for a project |
| Leave panes on `jocasta-<pid>` only | Nothing to build | Threads miss the tools and tasks his shell has |

## Recommendation
Add the name when the file exists.

## Choice
His answers of 2026-11-26 and the approved plan:
- When mise's global config folder (as the pane's environment resolves it) holds `config.<name>.toml`, where `<name>` is the name of the folder that holds the repository's git common dir (the main checkout's folder, the same for every worktree), `MISE_ENV` is `<what the pane had>,<name>,jocasta-<project id>`, each name once.
- Without the file, or outside git, `MISE_ENV` is as before.
- Windows, Linux and macOS resolve the folder the same way.

## Reason
A thread sees the same project tools his own shell does, and Jocasta's per-project override still wins over them.

## Consequences
- `mise.rs`: `jocasta_env` asks git for the common dir and checks the file before every mise call it prepares; `with_jocasta_env` takes the repository's name.
- `setup::mise_config_dir_in` resolves the folder from a given environment; `mise_config_dir` is it over the process's.
