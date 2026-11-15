---
name: mise-setup
description: Offering a project its Jocasta mise config (tools, env, test and verify tasks kept outside the repo). Use before a project's first thread when mise is installed and the project has no Jocasta mise config yet.
---

# A project's Jocasta mise config

Every project can have a mise config Jocasta keeps on his machine, never in the repo:
`~/.config/mise/config.jocasta-<project id>.override.toml` (the id is in parentheses in `projects()`). Every pane of
that project loads it through `MISE_ENV=jocasta-<project id>`, so each thread gets the project's tools, env and tasks,
and the review runs its `verify` task as the project's own check.

## When to offer

When he asks for work on a project, `mise --version` answers, and that file doesn't exist: offer once, in one line,
"<project> has no mise setup yet; shall I start a thread to write one?" Don't offer again for that project unless he
asks. Without mise, never offer.

## The setup thread

On his yes, `thread_start(project, "mise setup", brief)` with this brief, filled in:

> Write Jocasta's mise config for this project at `~/.config/mise/config.jocasta-<project id>.override.toml`, the only
> file you write; don't change the repo. Find the tools and versions the project uses (its version files, package
> manager, CI) and pin them exactly in `[tools]`. Put the env a run needs in `[env]`, loading the project's `.env`
> through `_.file = "{{cwd}}/.env"` when it has one; the thread variables `JOCASTA_THREAD`, `JOCASTA_PORT_OFFSET` and
> `COMPOSE_PROJECT_NAME` are already set and can be used as `{{env.JOCASTA_PORT_OFFSET}}`. Add `[tasks.test]` (the
> project's tests) and `[tasks.verify]` (everything a change must pass: build, lint, tests). Never run `mise trust`,
> `mise use` or `mise install`; show `mise tasks ls` once the file is written. This task changes no repo file: make no
> commit (the commit step doesn't apply).
>
> Then offer him the scanners that fit, and ask per project; add none without his yes. Trivy (committed secrets and
> dependencies) and Semgrep (a web service) go in `[tools]` pinned exactly, each as a `verify` step. knip and jscpd (a
> TypeScript project) are npm packages: name them to him as dev dependencies for the project's own package manager to
> pin (this task writes no repo file), and on his yes add their commands to `verify`.

He approves its plan in its pane like any thread's. It commits nothing, so it never reports done: when its
"finished its turn" line arrives, report that line to him as the setup's outcome and offer to close the thread.
