# 0054 Config dir

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Date: 2026-10-04

## Question
Where is Jarvis's config dir?

## Facts
- Pi 1.0.1 `config.js`: the agent dir is `JARVIS_CODING_AGENT_DIR` if set, else `~/<piConfig.configDir>/agent`.
- The 0.2 build shipped only `piConfig.name`, so releases used `~/.pi/agent`.
- 0035 already put `private-names.txt` in `~/.jarvis/agent/`.

## Options
| Option | Pros | Cons |
|---|---|---|
| `~/.jarvis/agent` via `piConfig.configDir: ".jarvis"` | Home-relative like omp's `~/.omp/agent` | — |
| Under the data dir via `JARVIS_CODING_AGENT_DIR` | Config and data together | Env var set before Pi starts; data dir undecided until 1.1 |
| Stock `~/.pi/agent` | No change | Mixes with stock Pi |

## Recommendation
`~/.jarvis/agent`.

## Choice
`~/.jarvis/agent`, through `piConfig.configDir: ".jarvis"` in the `package.json` shipped next to the exe.

## Reason
Not given.

## Consequences
`JARVIS_CODING_AGENT_DIR` still overrides it (Pi's own env var). Dev runs through `bun src/entry.ts` read Pi's package.json and use `~/.pi/agent`.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
