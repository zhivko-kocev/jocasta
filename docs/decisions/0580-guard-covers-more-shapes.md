# 0580 The guard covers more shapes, and done_when goes through it

- Status: v1
- Kind: product, security
- Phase/slice: program
- Vision: V8
- Date: 2026-11-09

## Question
The review found that `done_when` commands run outside the guard, the credential rules name files omp no longer uses, and several destructive or exfiltrating shapes pass the guard while some everyday git commands are denied. What does the guard cover?

## Facts
Verified 2026-11-09 by the review (0573), reports SEC-LOC (SEC-LOC-4, SEC-LOC-5), PRM (PRM-1, PRM-12, PRM-13) and TST (TST-9).
- Each `done_when` becomes a check spec run by the service through cmd.exe or sh (`checks.ts:113`); no check spec is matched against the guard, and `jocasta thread new --done-when` asks nothing (0544).
- `config/guard.yml:69-72` deny `auth.json` and `accounts.json`; omp 18.18.2 keeps credentials in `agent.db`, and `omp auth export`, `cat ~/.omp/agent/agent.db` and `sqlite3 … auth_credentials` are allowed.
- Allowed today: `git push origin HEAD:main`, `gh pr merge 12 --squash --admin`, `git push -fu origin main`, `GIT push --force`, `git restore --worktree .`, `git update-ref refs/heads/main <sha>`, `git credential fill`; omp's critical shapes (`bash <(curl …)`, `eval "$(curl …)"`, `| zsh`, `sudo rm`) are allowed under yolo; `*graphiti-token*` matches no file.
- Denied by mistake: `git diff --cached -- src/alias.ts`, `git add src/config/alias.ts && git commit …` (the two `alias.*` rules).
- `test/config/guard.test.ts` keeps its own matcher copy, which 0501 records as differing from omp's.

## Options
| Item | Options (pros / cons) |
|---|---|
| SEC-LOC-5 done_when | 1. guard-check at start, show the lines in any Allow box (no laundering / one more box line) · 2. guard-check only · 3. also make `jocasta thread new` ask cross-project (amends 0544) |
| PRM-1 credentials | (a) swap the stale names for `agent.db` and `omp auth export` · (b) also Claude Code's credentials file |
| Bypass shapes (SEC-LOC-4, PRM-13) | 1. add them as deny (closes cheap bypasses / a longer list) · 2. trunk pushes and `gh pr merge` as prompt, rest deny · 3. accepted risk |
| PRM-12 alias rules | (a) narrow the two alias rules · (b) also split the rm deny (still spans segments) |
| PRM-13 c publishing | prompt on publish commands (his call) · leave them |

## Recommendation
SEC-LOC-5 option 1 (0544's CLI door kept); PRM-1 (a); SEC-LOC-4 option 1 with PRM-13 (a) and (b); PRM-12 (a); PRM-13 (c) his call.

## Choice
- SEC-LOC-5 option 1: every `done_when` is checked with the guard's matcher (moved to `src/shared`, shared with the guard test and H1's evaluator) before it's stored, at both doors: deny refuses `thread_start` and `thread new`; prompt-level counts as needing his Allow; any Allow box shown lists the done_when lines ("It runs when the work is reviewed:  `bun test auth`"). `jocasta thread new` still asks nothing (0544 unchanged; the bench's CLI starts keep working).
- PRM-1 (a): the token block's four `auth.json`/`accounts.json` rules become `*omp auth export*`, `*.omp/agent/agent.db*` and `*.omp\agent\agent.db*` (covering `-wal`/`-shm`); guard.test.ts follows. Claude Code's credentials are not added.
- Bypass shapes (SEC-LOC-4 option 1, PRM-13 (a), and (b)'s visibility rule), deny, each with a corpus line in guard.test.ts and H1: combined force-flag clusters (`*git*push* -fu*`, `-uf`, … or the `-?f*`/`-f?*` forms); case variants of the program (`*GIT*`/`*Git*` copies of the destructive git rules); `*git*restore*--worktree*`, `*git*restore* -W*`, `*git*restore*--source*`; `*git*update-ref*refs/heads/*`; `*git*credential*fill*`, `*git*credential-manager*get*`; `*docker*inspect*jocasta*`, `*docker*exec*jocasta-*`, `*docker*exec*falkordb*`; omp's critical shapes `*<(curl*`, `*<(wget*`, `*eval $(curl*`, `*eval $(wget*`, `*| zsh*`, `*|zsh*`, `*| fish*`, `*sudo rm *`; `*gh repo edit*--visibility*`. The dead `*graphiti-token*` rule goes.
- Trunk: `*git*push* *:main*` and `*git*push* *:master*` deny; `*gh pr merge*` prompt (refused in subagents).
- PRM-12 (a): `*git*config*alias.*` becomes `*git*config *alias.*`; `*git*-c*alias.*` becomes `*git*--config-env*alias.*`; `*git*-c alias.*` stays; the rm deny and `git clean -n` stay as accepted noise.
- Publishing (PRM-13 (c)): prompt on `*npm*publish*`, `*pnpm*publish*`, `*bun*publish*`, `*cargo*publish*` and `*gh release create*`.
- The guard tests run through bench/h1-guard's evaluator (TST-9; omp's real resolver where available) instead of a private matcher copy.

## Reason
"Option 1" for SEC-LOC-5 and SEC-LOC-4; "(a)" for PRM-1, PRM-12 and PRM-13's critical shapes; the trunk rules as recommended (push deny, `gh pr merge` prompt); "(c)" for publishing.

## Consequences
- Amends 0450 (the deny list grows with omp's critical shapes), 0485 (the guard's list), 0489, 0540 (`done_when` is guard-checked before it's stored) and leaves 0544 as decided (no Allow at the CLI door).
- H1's corpus gains a line per new rule, so H1 runs on the new list.
