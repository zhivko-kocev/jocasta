# 0094 Where client-naming config lives

- Status: decided
- Kind: product
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
Where do scopes, watched roots and anything naming clients live?

## Facts
Verified 2026-10-07.
- The config dir is `~/.jarvis/agent` (0054). It holds `auth.json`, `settings.json`, Pi's `sessions/` and `bin/`, the identity `AGENTS.md`, the persona and `private-names.txt` (0035, 0055, 0066, 0069). It is local and never published.
- The Jarvis repo is public (0035); principle 6 keeps client names local.
- v4 put them in a separate private dir because its config store was a public repo; that reason no longer applies.

## Options
| Option | Pros | Cons |
|---|---|---|
| Jarvis's config dir `~/.jarvis/agent` | One place for everything he edits; already private; no new path | A Pi package or tool that reads the whole config dir sees client names too |
| A separate private dir, `%AppData%\jarvis` and the OS equivalents | Client-naming files apart from Pi's tree | A third location; v4's reason is gone |
| Inside the data dir | All Jarvis-owned state together | Hand-edited YAML mixed with machine-written DBs |

## Recommendation
The config dir.

## Choice
The config dir, `~/.jarvis/agent`.

## Reason
Not given.

## Consequences
- The roots file (1.2) and the scopes file (1.3) live in `~/.jarvis/agent`; their names are decided there.
- Section 7's Locations convention: the third location collapses into the config dir.
