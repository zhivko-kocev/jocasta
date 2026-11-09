# 0189 Slice 2.2 agenda

- Status: decided
- Kind: process
- Phase/slice: 2/2.2
- Date: 2026-10-07

## Question
Which decisions does slice 2.2 (git-guard and corpus) grill, now that v4 2.2 and the omp plan's seeds are decided fresh (0088)?

## Facts
Verified 2026-10-07 in Pi 1.0.1: a throwing `tool_call` handler blocks the tool (`dist/core/agent-session.js:319-333`); a throwing `user_bash` handler blocks the `!` command (`dist/core/extensions/runner.js:973-999`); Pi keeps per-folder trust in `<config dir>/trust.json` (`dist/core/trust-manager.js:171-177`).
- Lessons mapped to phase 2: L04 (2.6), L24 (phase 2), L25 (2.1, 2.4), L26 (2.1, 2.2), L27 (2.1), L28 (2.5), L36 (2.7), L43 (2.1), L48 (2.1), L68 (2.1's parser dossier).
- Lesson L26's hand parser passed its 52-case corpus on this machine; its gotchas: Git Bash rewrites `..\.env`, cmd reads it raw, `del /s` isn't cmd in Git Bash.
- The exit smoke names rule ids like `git-guard/push-force`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of ten: implementation; base rule set; corpus seeds; five disagreements (`rm -r` without `-f`, `sudo`, `mkfs`, `dd of=/dev/…`, asking before `git add`); Windows shells; a broken rules or corpus file; plus the rule id format and denial message | Each disagreement is its own question | Ten questions |
| Change it | — | — |

## Recommendation
The fresh agenda.

## Choice
The fresh agenda of ten.

## Reason
Not given.

## Consequences
- Plan 2.2's list becomes the agenda, citing this record.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
