---
name: mise
description: Run a project's mise tasks and pinned tools without changing mise's trust, pins or settings. Use when a repo has mise.toml, .mise.toml, mise/ or .config/mise*, or a command fails through a mise shim.
---

# mise in Jocasta's panes

mise pins a project's tools and runs its tasks. Jocasta set it up so that nothing about it changes without Zhivko (0476): its machine file is written only by `jocasta setup`, and trust is his call. The one exception is Jocasta's own: before a thread's pane starts, it trusts the worktree's unchanged copy of a config he trusted in the main checkout (0596). A copy the thread changed is untrusted again until he trusts it.

## Use

- When the project has `mise.toml`, `.mise.toml`, `mise/` or `.config/mise*`, list its tasks with `mise tasks ls --json`.
- Prefer `mise run <task>` for check, test and build when such a task exists. The project's `verify` skill, when it has one, comes first.
- Tools on PATH already resolve through mise's shims: run them directly, never wrapped in `mise exec`.
- When mise says a config isn't trusted, or a pinned tool isn't installed, stop and tell him what it said. Trusting a config and installing tools are his.

## Read only, for diagnosis

`mise ls --current`, `mise which <tool>`, `mise config ls`, `mise doctor`, and `mise env --json` (env may hold secrets: never print its values).

## Never

- `mise trust` in any form, or `MISE_TRUSTED_CONFIG_PATHS`.
- `mise use` in any form (above all `-g` or `--global`), `mise upgrade` or `--bump`, `mise install` of a version no config pins, `mise settings set`.
- `--yes` or `MISE_YES=1`.
- Editing anything under `~/.config/mise`, except the one file `config.jocasta-<project id>.override.toml` when a Jocasta brief asks for it (0477). Jocasta's `conf.d/jocasta.toml` changes only through `jocasta setup`.
- `omp update` when omp is installed through mise: it runs `mise upgrade … --bump` and rewrites the pin.
