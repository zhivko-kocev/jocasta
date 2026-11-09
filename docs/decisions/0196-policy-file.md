# 0196 Policy file

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
Where do guard rules live?

## Facts
Verified 2026-10-08: Pi 1.1.0 has no permission rules of its own.
- Principle 5: YAML for Jarvis's config; the config dir holds files he edits (0094).
- His Claude Code setup: `Bash(*)` allowed plus about 100 deny rules (appendix A).

## Options
| Option | Pros | Cons |
|---|---|---|
| `permissions.yaml` in the config dir, plus an optional project `.jarvis/permissions.yaml` gated by trust (2.3) | One owner; Jarvis's format | A new file |
| Inside Pi's `settings.json` | One file | Mixes Pi's JSON with Jarvis's rules |

## Recommendation
`permissions.yaml`.

## Choice
Now (0220): the project file loads in every project, trusted or not.

Originally:
`~/.jarvis/agent/permissions.yaml`, plus an optional project `.jarvis/permissions.yaml` that loads only in a trusted project (2.3).

## Reason
Not given.

## Consequences
- The rule grammar (item 2) is confirmed with the next questions; the question's preview was illustrative.
- Amended by 0220: the project file loads in every project, trusted or not.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
