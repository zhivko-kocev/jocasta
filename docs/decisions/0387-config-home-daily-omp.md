# 0387 The config home is his daily omp

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Whose config is the harness's?

## Facts
Verified 2026-10-13.
- omp keeps its config, sessions and logins in `~/.omp/agent` (`config.yml`, `agent.db`, `sessions/`).
- Jarvis keeps `projects.yaml`, `compose.yaml`, `containers/` and Pi's `settings.json` in `~/.jarvis/agent`.

## Options
| Option | Pros | Cons |
|---|---|---|
| His daily omp (`~/.omp/agent`) | One omp, one config, his existing settings, sessions and logins; the command is `omp` | Jarvis's sessions mix with his daily ones |
| Separate `~/.jarvis/agent` | Jarvis runs apart from his daily omp | Two configs and two logins to keep |

## Recommendation
His daily omp.

## Choice
The harness config is his daily `~/.omp/agent`; the harness command is `omp`. Jarvis-only files stay in `~/.jarvis/agent` (`projects.yaml`, `compose.yaml`, `containers/`, `herdr.toml`), and Jarvis-only settings (`frontier.*`, `pollers.*`) move from Pi's `settings.json` to `~/.jarvis/agent/jarvis.yaml`. `jarvis` opens the work surface; `jarvis setup`; `jarvis mcp`; `jarvis project` stays; `jarvis thread`, `guard`, `memory import` and `harness` retire.

## Reason
"Your daily omp (~/.omp/agent)".

## Consequences
- Writes into `~/.omp/agent` happen only through `jarvis setup` after his `y`, or by him.
- Appendix I lists the new files and commands and the retired ones.
- Amended by 0491: bare `jocasta` exits 0 when the companion stopped herdr's server itself.
