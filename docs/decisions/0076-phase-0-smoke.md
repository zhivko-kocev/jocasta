# 0076 Phase 0 smoke

- Status: decided
- Kind: process
- Phase/slice: 0/0.8
- Date: 2026-10-04

## Question
How is phase 0 verified?

## Facts
- The phase 0 exit smoke, as decided: `--version` names three pins; the TUI opens under Jarvis's name with `AGENTS.md` from `~/.jarvis/agent`; `/login` writes `~/.jarvis/agent/auth.json` (0063); "who am I, and who are you?" answers from 0066–0068. The guard check doesn't apply (0065).
- Sign-in and the identity question need Zhivko's account and the network.

## Options
| Option | Pros | Cons |
|---|---|---|
| `scripts/smoke-phase0.ps1` | Repeatable at every Pi bump | A script to keep |
| The steps in the docs only | Nothing to keep | By hand every time |

## Recommendation
The script.

## Choice
`scripts/smoke-phase0.ps1`: runs typecheck, lint, tests and the windows-x64 build; checks `--version` against the three pins; runs the compiled exe with `--list-models` in a temp home and checks the config dir, the seeded settings and that an existing `settings.json` is left alone; prints the manual steps.

## Reason
Not given.

## Consequences
- The header check is a printed manual step, not automated: it needs a real terminal. The question's preview said the script would check it.
- Manual steps: the header and context list, `/login`, and the identity question.
