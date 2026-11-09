# 0055 Config dir contents and owners

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Date: 2026-10-04

## Question
What does the config dir hold, and who edits each file?

## Facts
The 0.2 smoke showed Pi writing `models-store.json`, `sessions/` and `bin/` (downloaded tools) into the config dir. 0035 added `private-names.txt`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Approve the preview | One owner per file (principle 3) | — |
| Change it | — | — |

## Recommendation
Approve.

## Choice
| File | Edited by |
|---|---|
| `AGENTS.md`, `skills/`, `prompts/`, `mcp.json` | Zhivko |
| `private-names.txt` | Zhivko (0035) |
| `settings.json` | Zhivko, and Pi for settings changed in its UI; Jarvis writes it once if missing (0056) |
| `auth.json` (only if 0.4 keeps file storage), `trust.json`, `mcp-auth.json`, `models-store.json`, installed packages, `bin/` | Jarvis and Pi |
| `sessions/` | Jarvis and Pi, until phase 1 moves history under threads (0057) |

## Reason
Not given.

## Consequences
- `compose.yaml` joins the Jarvis-edited files when the first container resource lands (0072).
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
