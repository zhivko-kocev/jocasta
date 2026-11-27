# 0708 Clean-room runs

- Status: v1 (built 2026-11-27)
- Kind: product, security
- Phase/slice: ultron 0.3 iteration, M2 R1
- Date: 2026-11-26

## Question
Review v2 runs a change's checks before anyone judges it (0706). Other people's pull requests must not run on his machine as him, and his own threads' checks pass on a worktree full of his local state that a fresh clone doesn't have. Where and how does Jocasta run a check that sees only the committed tree?

## Facts
- Docker exists on his Linux PC; Docker Desktop on Windows and macOS. The CI runners for Windows and macOS have none.
- `jdxcode/mise` is mise's official image on Docker Hub (pulling it is approved by the plan); mise installs a project's tools from its `mise.toml`, and `MISE_TRUSTED_CONFIG_PATHS` trusts a folder for one process without `mise trust`.
- A project without a `mise.toml` of its own keeps its tools and tasks in `~/.config/mise/config.<repo folder>.toml` (0704) and Jocasta's `config.jocasta-<project id>.override.toml` (0477). mise loads system config from `/etc/mise/conf.d/*.toml`.
- `git archive <commit>` writes exactly the committed tree; nothing untracked or ignored.
- A container's `docker run` client can be killed while the container goes on; a named container is removed with `docker rm -f`.
- A container running as root that writes into a bind-mounted host folder leaves root-owned files under rootful Docker.

## Options
| Option | Pros | Cons |
|---|---|---|
| `git archive` into a temp folder, mounted read-only into `jdxcode/mise` and unpacked inside; per-project cache volume; the project's mise files mounted read-only | Sees only the commit; nothing of his reaches the container; tools cached across runs; no host folder is written by the container | Docker needed; first run downloads the image and the tools |
| A fresh `git worktree` on the host | No Docker | Runs as him with his whole home; no isolation for strangers' code |
| A custom image per project | Fast | An image to build and keep per project |

## Recommendation
`git archive` into `jdxcode/mise`, cache volume per project, the mise files mounted read-only.

## Choice
His answers of 2026-11-26 and the approved plan:
- `src/frontier/server/cleanroom.rs` runs a command on a commit: `git archive --format=tar --output <temp>/src.tar <commit>`, then `docker run --rm --init --name jocasta-cr-<random> --mount type=bind,source=<temp>/src.tar,target=/src.tar,readonly -v jocasta-cache-<project id>:/cache -e MISE_DATA_DIR=/cache/mise -e MISE_CACHE_DIR=/cache/mise-cache -e CARGO_HOME=/cache/cargo -e npm_config_cache=/cache/npm -e PIP_CACHE_DIR=/cache/pip -e GOMODCACHE=/cache/go/mod -e GOCACHE=/cache/go/build -e MISE_TRUSTED_CONFIG_PATHS=/work -e MISE_YES=1`, plus read-only mounts of `~/.config/mise/config.<repo folder>.toml` at `/etc/mise/conf.d/10-project.toml` and `config.jocasta-<project id>.override.toml` at `/etc/mise/conf.d/20-jocasta.toml` when they exist, image `jdxcode/mise:latest`, `-w /work`, command `sh -lc 'mkdir -p /work && tar -xf /src.tar -C /work && mise install && <command>'`. No home folder, no host environment variable, no Docker socket goes in. The temp folder is removed afterwards; past the time limit the container is removed with `docker rm -f`.
- Built detail: the commit goes in as a read-only tar unpacked inside the container (not a bind of the folder), so the container never writes a host folder and Windows paths need no `-v` drive-letter parsing (`--mount` takes them whole).
- The check is the project's `verify` task, else `test`, found inside the container after `mise install` (`mise tasks info`), so a stranger's config is never loaded by the host's mise. The run's result is `{exit, tail, ms}` (the last 20 lines of its output); its time limit is a check's, 30 minutes.
- Results are kept per committed tree and mise files (`<data>/checks/<tree 12>/cleanroom-<hash>.json`), pass and fail alike, and pruned with the other check results.
- Docker answers when `docker info` does within 5 s. The image is pulled when it isn't there (15 minutes at most).
- `jocasta.yaml` `checks.cleanRoom: auto | off` (default `auto`); `projects.yaml` `cleanRoom: {<repo folder>: off}` turns it off for one repository (keyed like `push:`, 0613).
- "Not possible", never an error for his own threads, with its reason kept for the page: Docker isn't running, clean-room runs are off, the image can't be pulled, or the project has neither a `verify` nor a `test` task.

## Reason
Strangers' code runs where it can reach nothing of his, and a change is judged on what was committed, not on what happens to sit in a worktree.

## Consequences
- New module `cleanroom.rs` (`available`, `run`, `check`, `Check`, `Ran`); `mise.rs` `repo_mise_name` is shared with it; `settings.rs` reads `checks.cleanRoom`; `projects_file.rs` reads `cleanRoom`.
- Its tests that need Docker skip and say why when `docker info` fails (the Windows and macOS CI runners).
- Docker's cache volume `jocasta-cache-<project id>` stays between runs; removing it is `docker volume rm`.
