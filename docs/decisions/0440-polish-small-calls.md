# 0440 Polish round: small calls

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
The polish round's review found six small calls beyond the records. Which are taken?

## Facts
Verified 2026-10-15.
- `install.ps1` stops the companion during an upgrade and never starts it again; whiteboard's old Node keeps running.
- `jarvis setup` writes `~/.jarvis/agent/herdr.toml` only when missing, so template changes never reach him and setup says "Nothing to set up".
- While disconnected the page dims everything to 0.55, dropping quiet text to 2.65:1 (0432).
- A merged thread's handoff summary needs its worktree to resolve a project; after a companion restart the worktree is gone and the summary is never sent to Graphiti (0389, 0400).
- `recognizer.test.ts` loads the real 622 MB dictation model whenever it is present.
- The polished build can be installed in place over his `%LOCALAPPDATA%\Programs\jarvis`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Restart the companion after an upgrade | He keeps a running companion | A started process from the installer |
| Setup refreshes herdr.toml with a diff | Template changes reach him; he sees them first | One more question in setup |
| Offline keeps full text contrast | Readable while reconnecting | Staleness shown less loudly |
| Summaries keep their project | Handoffs reach Graphiti after a restart | catalog.db v7 |
| Dictation smoke opt-in | Fast, isolated tests | Real model checked only on request |
| Install in place at the end | He runs the polished build | His install changes |

## Recommendation
All six.

## Choice
(1) after an upgrade stopped the companion, `install.ps1` starts it again (and first stops whiteboard's old Node); (2) `jarvis setup` shows the diff of a `~/.jarvis/agent/herdr.toml` that differs from the shipped template and replaces it after `y`, like `mcp.json` (omp's `config.yml` stays write-when-missing, 0420); (3) while disconnected the page keeps full text contrast and shows staleness only by the status line and dimmed state marks; (4) `catalog.db` v7 adds `summaries.project_id`, set when the summary is stored, so a merged thread's handoff still reaches Graphiti after a companion restart; (5) the test that loads the real dictation model runs only with `JARVIS_DICTATION_SMOKE=1`; (6) the polished build is installed in place at the end.

## Reason
Ticked all six.

## Consequences
- Amends 0432 (offline dim), 0389 and 0400 (summaries).
- Adds the test-only env var `JARVIS_DICTATION_SMOKE` and migration `catalog.db` v7 (plan appendix I).
- Amended by 0466: herdr.toml's replacement is one of setup's numbered migrations, shown as a diff and applied after his `y`.
- Amended by 0467: `jarvis setup --refresh <file>` resets herdr.toml with a `.bak` backup and reloads herdr.
