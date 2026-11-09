# 0098 Dir overrides

- Status: decided
- Kind: product
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
Which env vars override Jarvis's dirs?

## Facts
Verified 2026-10-07.
- Pi 1.0.1 reads `JARVIS_CODING_AGENT_DIR` for the config dir and `JARVIS_CODING_AGENT_SESSION_DIR` for sessions (`dist/config.js:438-439`), because `piConfig.name` is "jarvis" (0054).
- The plan's first option was `JARVIS_DATA_DIR` and `JARVIS_CONFIG_DIR`; appendix I claimed both.
- The 0.8 smoke already isolates runs with a temp home (0076).
- What changed (0004): the config dir override already exists in Pi, so `JARVIS_CONFIG_DIR` would be a second name for it.

## Options
| Option | Pros | Cons |
|---|---|---|
| `JARVIS_DATA_DIR`, and Pi's `JARVIS_CODING_AGENT_DIR` for the config dir | One new knob; tests and smokes point both dirs at a temp dir | The two names differ in style |
| `JARVIS_DATA_DIR` and `JARVIS_CONFIG_DIR`, copied into Pi's var by the entry | Matching names | Two names for one setting, and a precedence rule when both are set (principle 1) |
| None | No knobs | The data dir can't move without moving the home |

## Recommendation
`JARVIS_DATA_DIR`, and Pi's existing var for the config dir. This changes the plan's first option because of the fact above.

## Choice
`JARVIS_DATA_DIR` overrides the data dir; `JARVIS_CODING_AGENT_DIR` (Pi's) overrides the config dir. There is no `JARVIS_CONFIG_DIR`.

## Reason
Not given.

## Consequences
- Appendix I's env vars: `JARVIS_DATA_DIR` (1.1); `JARVIS_CONFIG_DIR` is freed; Pi's `JARVIS_CODING_AGENT_DIR` and `JARVIS_CODING_AGENT_SESSION_DIR` are listed as Pi-owned names.
