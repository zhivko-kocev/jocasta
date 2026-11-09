# 0398 omp settings for Jarvis

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Which omp settings does Jarvis rely on, from the walkthrough (0396), and what is the guard's configuration (0391)?

## Facts
Verified 2026-10-13.
- Guide: `docs/research/omp-adaptation.md` (550 registered settings; recommended deltas; network destinations).
- Under `tools.approvalMode: yolo` (omp's default) omp's built-in dangerous-command patterns don't prompt (`tools/approval.ts:200-201,255-272`); `bash.patterns` deny/prompt rules and `tools.approval.<tool>` still act; subagents reject `prompt` (`docs/approval-mode.md:172`). The built-in patterns are POSIX-only.
- `bash.patterns` items are `{match, approval: allow|deny|prompt}`, first match wins, `*` is the only wildcard, matched per command segment (`tools/bash.ts:232-306`). `bash.gitGuard` refuses stash, `reset --hard` and checkout/switch/restore outside a conflict (`exec/settings.ts:121`).
- `telemetry.otlpExportEnabled` (default true) sends nothing without an `OTEL_*` endpoint; `dev.autoqa` (default true) reports tool issues to `qa.omp.sh` after consent; `startup.checkUpdate` asks npm on interactive start; `marketplace.autoUpdate` checks added marketplaces each launch; `recap.enabled` writes an idle recap with a model call; `providers.cacheWarming: idle` sends keep-warm requests while idle; `paste.largeMenuThreshold` (100) opens a menu on large pastes, and herdr sends prompts as bracketed pastes.
- His daily `~/.omp/agent/config.yml` already exists, so `jarvis setup` will not overwrite it (0396).

## Options
| Option | Pros | Cons |
|---|---|---|
| Guard: `write` + `bash.gitGuard` | Built-in dangerous patterns prompt again; writes ask | More prompts in unattended threads |
| Guard: `yolo` + deny rules + `bash.gitGuard` | No prompts except his rules; carries his decided Jarvis rules (0210, 0218, 0382) | Commands outside the rules run unasked |
| Guard: `always-ask` | Everything asks | Unusable for threads |

## Recommendation
Guard `write` + `bash.gitGuard`; the ticked deltas.

## Choice
Guard: `tools.approvalMode` stays `yolo`, `bash.gitGuard: true`, and `bash.patterns` (in order): prompt `git push* --force-with-lease*`; deny `git push* --force*`, `git push* -f*`, `git reset --hard*`, `git clean -*f*`, `git branch -D*`, `gh repo delete*`; prompt `rm -rf*`, `rm -fr*`, `Remove-Item*-Recurse*`, `git stash drop*`, `git stash clear*`, `git reflog expire*`, `git gc --prune=now*`, `*| sh*`, `*| bash*`, `*| iex*`, `*Invoke-Expression*`. Deltas: `telemetry.otlpExportEnabled: false`, `dev.autoqa: false`, `dev.autoqaConsent: denied`, `startup.checkUpdate: false`, `startup.quiet: true`, `marketplace.autoUpdate: off`, `recap.enabled: false`, `providers.cacheWarming: streaming`, `lsp.diagnosticsOnEdit: true`, `paste.largeMenuThreshold: 0`. Memory and compaction values are 0399 and 0400. He sets them himself in `/settings`; the rest of his `/settings` pass comes later and its non-defaults are added here then. `config/omp-config.yml` holds these values.

## Reason
"yolo + deny rules", then "Same, recursive deletes prompt instead of deny"; the deltas by tick; "I set them in /settings"; "later" for the rest of the pass.

## Consequences
- Differs from the recommendation (`write`).
- `secrets.enabled`, `personality` and the model roles were not chosen; they stay as his daily config has them.
- His personal rules (`~/.jarvis/agent/AGENTS.md`) don't move into omp for now; the Jarvis persona goes only into the boss pane's appended manual (0392 as decided), so threads and his daily omp stay plain.
- Amended by 0450: the guard's corrected list moves to `config/guard.yml` in every pane; under yolo only `deny` and non-critical `prompt` rules act.
- Amended by 0451: the reference config gains more values (disabled Claude plugins, web, find, startup, github, collab, secrets) and documented keybindings, `WATCHDOG.md` and `lsp.json`.
