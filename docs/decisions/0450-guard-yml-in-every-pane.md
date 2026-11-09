# 0450 The guard in every pane: config/guard.yml

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
0398's guard patterns live only in the reference config he sets by hand (0420), and several commands slip past them. Where does the guard live, and which rules does it hold?

## Facts
Verified 2026-10-23.
- His live config has no `bash:` key, so no guard is active (`~/.omp/agent/config.yml`, grep); setup writes into a missing `config.yml` only (`src/setup/setup.ts:240-244`).
- omp's matcher is anchored, case-sensitive, `*` only (`tools/bash.ts:241-251`); quotes are stripped and segments of compound commands are checked for deny/prompt (`bash.ts:283-297`); the first rule in list order wins (`bash.ts:311-316`).
- For a single command omp's built-in critical list returns before prompt rules (`bash.ts:549-552`), and yolo never prompts it (`approval.ts:384-401`): `rm -rf /tmp/x` and `curl -s https://x | sh` ran with no prompt under 0398's list.
- `bash.patterns` and gitGuard don't apply to `eval` (omp `docs/approval-mode.md`); `tools.approval.eval: prompt` asks on every eval cell and rejects eval in every subagent (same doc, §Subagents).
- `--config` can repeat (`cli/flag-tables.ts:122-124`), and overlays outrank a repo's `.omp/config.yml` (`config/settings.ts:3911-3914`).
- omp has no classifier mode: its modes are `always-ask`, `always-ask-cached`, `write`, `yolo` (`approval.ts:44`).
- A `tool_call` extension can block, fail-closed, and sees eval input (omp `docs/extensions.md:83-86`).
- The corrected list below was run through omp's own matcher under yolo against a corpus of deny, prompt and allow commands; every result matched (`local://verified-VerifySafety.md` §2).

## Options
| Option | Pros | Cons |
|---|---|---|
| Corrected rules in `config/guard.yml`, passed to every pane | Active whatever his global config says; outranks repo config; closes the holes the corpus showed | Case-sensitive globs miss mixed-case PowerShell and aliases; eval stays uncovered |
| Extension guard (`tool_call` hook) | Case-insensitive; unwraps wrappers; sees eval | Jarvis code inside omp (reverses 0391); re-checked on every omp bump |
| Per-call eval prompt (`tools.approval.eval: prompt`) | Covers eval | Asks on every eval cell; rejects eval in every subagent |

## Recommendation
Corrected rules in `config/guard.yml`, passed to every pane; critical shapes refused; the eval gap accepted.

## Choice
Jarvis ships `config/guard.yml` (beside `jarvis.exe`) with `bash.gitGuard: true` and the corrected `bash.patterns` list (32 deny, 25 prompt, deny before prompt; shapes on omp's critical list — `/`-rooted recursive rm, `--no-preserve-root`, piping into sh/bash — are deny). Every project, thread and boss pane starts with an extra `--config <config>/guard.yml`. The `bash:` block leaves `config/omp-config.yml`. Shell commands run through omp's `eval` tool aren't covered: accepted and written down (no per-call eval prompt). An omp extension guard is not built.

The list:
```yaml
bash:
  gitGuard: true
  patterns:
    # deny: git and gh (the leading `*git *` also catches `git -C x`, `cd x &&`, `pwsh -c "…"`, `cmd /c`)
    - { match: "*git *push* --force", approval: deny }
    - { match: "*git *push* --force *", approval: deny }
    - { match: "*git *push* -f", approval: deny }
    - { match: "*git *push* -f *", approval: deny }
    - { match: "*git *push* +*", approval: deny }
    - { match: "*git *reset --hard*", approval: deny }
    - { match: "*git *clean -*", approval: deny }
    - { match: "*git *branch -D*", approval: deny }
    - { match: "*git *checkout .*", approval: deny }
    - { match: "*git *checkout -- *", approval: deny }
    - { match: "*git *checkout -f*", approval: deny }
    - { match: "*git *checkout --force*", approval: deny }
    - { match: "*git *restore .*", approval: deny }
    - { match: "*git *restore -- *", approval: deny }
    - { match: "*git *switch -f*", approval: deny }
    - { match: "*git *switch --force*", approval: deny }
    - { match: "*git *switch --discard-changes*", approval: deny }
    - { match: "*git *worktree remove* -f*", approval: deny }
    - { match: "*git *worktree remove* --force*", approval: deny }
    - { match: "*gh repo delete*", approval: deny }
    # deny: shapes omp's critical list also matches (a prompt would be silently allowed under yolo)
    - { match: "*rm -*r* /*", approval: deny }
    - { match: "*rm -*R* /*", approval: deny }
    - { match: "*--no-preserve-root*", approval: deny }
    - { match: "*shutdown /*", approval: deny }
    - { match: "*| sh", approval: deny }
    - { match: "*| sh *", approval: deny }
    - { match: "*|sh", approval: deny }
    - { match: "*|sh *", approval: deny }
    - { match: "*| bash", approval: deny }
    - { match: "*| bash *", approval: deny }
    - { match: "*|bash", approval: deny }
    - { match: "*|bash *", approval: deny }
    # prompt
    - { match: "*git *push* --force-with-lease*", approval: prompt }
    - { match: "*rm -r *", approval: prompt }
    - { match: "*rm -R *", approval: prompt }
    - { match: "*rm -rf *", approval: prompt }
    - { match: "*rm -fr *", approval: prompt }
    - { match: "*rm -Rf *", approval: prompt }
    - { match: "*rm -fR *", approval: prompt }
    - { match: "*rm --recursive*", approval: prompt }
    - { match: "*rm -* -r *", approval: prompt }
    - { match: "*Remove-Item* -R*", approval: prompt }
    - { match: "*Remove-Item* -r*", approval: prompt }
    - { match: "*remove-item* -r*", approval: prompt }
    - { match: "*remove-item* -R*", approval: prompt }
    - { match: "*rd /s*", approval: prompt }
    - { match: "*rd /S*", approval: prompt }
    - { match: "*rmdir /s*", approval: prompt }
    - { match: "*rmdir /S*", approval: prompt }
    - { match: "*git *reflog expire*", approval: prompt }
    - { match: "*git *gc --prune=now*", approval: prompt }
    - { match: "*Invoke-Expression*", approval: prompt }
    - { match: "*invoke-expression*", approval: prompt }
    - { match: "*| iex*", approval: prompt }
    - { match: "*|iex*", approval: prompt }
    - { match: "*iex (*", approval: prompt }
    - { match: "*iex(*", approval: prompt }
```

## Reason
"Corrected rules per pane"; "refuse instead of ask for critical shapes"; "accept the eval gap".

## Consequences
- Amends 0391 (the guard ships as a `--config` overlay in every pane) and 0398 (the list; its fact that deny/prompt rules still act under yolo holds only for `deny` and non-critical `prompt`).
- 0398's `git stash drop*`/`git stash clear*` rules are dropped (gitGuard refuses all stash but list/show), and its `Remove-Item*-Recurse*` rule, which never fired.
- Known limits, written down: a commit message quoting a denied command is denied; mixed-case PowerShell, the `ri`/`del`/`erase` aliases, interpreter one-liners and `eval` are not covered; prompts are rejected in subagents.
- Plan appendix I names `config/guard.yml` and the extra `--config`.
- Amended by 0476: the list adds deny rules for `mise trust`, `mise use -g` and `mise upgrade … --bump`.
- Amended by 0485: the list adds rules for the known bypasses and token exfiltration; what patterns can't stop is an accepted risk in the README and the thesis; OS isolation for thread panes after v1.
