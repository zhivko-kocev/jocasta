# 0102 Where projects come from

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
Where do projects come from?

## Facts
- The roots file lives in `~/.jarvis/agent` (0094).
- v4 registered a project the first time Jarvis ran in a directory.
- The fixture gate (0026) and every smoke run Jarvis in temp dirs, which auto-registration would add to the catalog.
- What happens in a folder that is not a project is 1.4's item 11.

## Options
| Option | Pros | Cons |
|---|---|---|
| Watched roots in a config file, plus manual add and remove | The catalog holds only what he chose | A repo outside every root needs `project add` once |
| Auto-registration on first run | Zero setup | Temp dirs, fixtures and one-off clones pile up |
| Both | Roots for the usual places, auto for strays | The same pollution as auto; two mechanisms |

## Recommendation
Watched roots plus manual add and remove.

## Choice
Watched roots listed in a file in the config dir, plus `project add` and `project remove`.

## Reason
Not given.

## Consequences
- No auto-registration.
