# 0650 The omp snapshot: setup writes Jocasta's omp files over his

- Status: v1 (built 2026-11-18, unattended; for his review)
- Kind: product
- Phase/slice: fork build, phase 6
- Date: 2026-11-18

## Question
The second part of Jocasta is "a snapshot of the most optimal state of omp". How does it reach his `~/.omp/agent`: as an omp plugin, an omp profile, a merge into his files, or a replacement of them?

## Facts
Verified 2026-11-18 (`docs/research/two-plugins.md`).
- An omp plugin can't ship settings as a file. An extension can only override them in memory.
- `omp --profile <name>` moves the whole user base, logins and sessions included. Only keybindings are inherited.
- Setup's ten omp migrations each edited one file in place, with markers and undo records of their own.

## Options
- **Replace:** back up the files the snapshot has, then write the snapshot over them. Pros: one known state, whole files. Cons: his keys outside the snapshot go to the backup.
- **Merge** into his files. Pros: his keys stay. Cons: the state Jocasta runs on varies per machine.
- **A `jocasta` profile.** Pros: his everyday omp is untouched. Cons: its own sign-in, and its sessions are kept apart.

## Recommendation
Replace.

## Choice
His answer of 2026-11-18: replace, chosen over a merge and over an omp profile. Built as `docs/research/fork-build.md` choices 49–56, 59 and 60 list:
- **The files.** The snapshot is `config/omp/`: `config.yml`, `keybindings.yml`, `lsp.json`, `WATCHDOG.md`, `secrets.yml` and `skills/mise/SKILL.md`. Two files are made per install: `mcp.json` (the `jocasta` server only, with this install's path) and herdr's omp hook from the fork's own asset.
- **One migration.** Migration 19, `omp-snapshot`, shows one diff and applies after one `y`.
- **What it touches.** `secrets.yml` is written only when missing. Logins, sessions, memory and history are never touched.
- **Backups.** Each replaced file is kept first in `~/.jocasta/agent/setup/omp-backup/<UTC stamp>/`.
- **Uninstall.** It gives back his file from before Jocasta while the file is still the snapshot's. A file he changed since stays, and his original is put beside it as `<name>.before-jocasta`.
- **What carries over.** The snapshot's `config.yml` keeps his decided model roles, fallback chains and `advisor.enabled`.

## Reason
His words: "just keeping the files basically like a snapshot, these are the most optimal settings already set up, the global skills needed and stuff like that — to keep the lightweight omp offers but still to make it perform better and cheaper"; "it doesnt have to be a plugin".

## Consequences
- His first `y` removes the keys of his not in the snapshot: his status line, `tui.textSizing`, `security.enabled`, `stt.enabled`, `speech.enabled` and `update.auto`. It also removes his other global MCP servers. All stay in the backup (`fork-build.md`, For him 12).
- The snapshot's values come from the settings trials (record 0651).
- Amends 0387 and 0396. Setup writes the snapshot's whole files in place of its per-file edits to his omp folder. Migrations 1, 3, 4, 6, 9–13, 15, 16 and 17 are removed, and their old markers are ignored. Migration 18 removes the old herdr plugin's link.
