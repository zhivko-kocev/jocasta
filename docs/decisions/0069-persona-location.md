# 0069 Where identity and persona live

- Status: decided
- Kind: product
- Phase/slice: 0/0.6
- Date: 2026-10-04

## Question
Which files hold the identity, rules and persona?

## Facts
Pi 1.0.1 `docs/configuration.md`: `<agent-dir>/SYSTEM.md` replaces Pi's system prompt; `<agent-dir>/APPEND_SYSTEM.md` adds to it; a trusted project's `.pi/` copy takes precedence. `AGENTS.md` loads as context (0.3 smoke).

## Options
| Option | Pros | Cons |
|---|---|---|
| Persona in `APPEND_SYSTEM.md`, identity and rules in `AGENTS.md` | Persona in the system prompt for every request | Two files |
| Everything in `AGENTS.md` | One file | Persona is context |
| `AGENTS.md` importing `rules/*.md` | Small files | Needs 1.9 |

## Recommendation
`APPEND_SYSTEM.md` and `AGENTS.md`.

## Choice
Persona in `~/.jarvis/agent/APPEND_SYSTEM.md`; identity and rules in `~/.jarvis/agent/AGENTS.md`.

## Reason
Not given.

## Consequences
A trusted project's `.pi/APPEND_SYSTEM.md` would replace the persona for that project (Pi's precedence).
