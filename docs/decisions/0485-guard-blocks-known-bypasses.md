# 0485 The guard blocks the known bypasses; OS isolation after v1

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
The security review's finding 1: untrusted content reaches yolo-mode agents, and the guard is a bypassable denylist. What does v1 do about it?

## Facts
Verified 2026-10-24.
- The guard is command-shape matching (`config/guard.yml:7-12,16-36,64-68`): every git rule needs the literal text `git `, and PowerShell recursion is caught only as `-r`/`-R` followed by a space.
- Bypasses: `git.exe …`, git aliases and `git -c alias.…`, `git push origin :main`, `--delete`, `--mirror`, `gh api -X DELETE`, PowerShell `rm -Recurse` (no space), `python -c`, omp's `eval`.
- Token exfiltration (`gh auth token | curl …`) isn't covered.
- Thread panes run omp in yolo mode as him.

## Options
| Option | Pros | Cons |
|---|---|---|
| Patch the guard now, OS isolation after v1 | The known bypasses and token reads are stopped in v1; no change to how threads work | Patterns can't stop scripts, interpreters or `eval`; the gap is written down, not closed |
| OS isolation in v1 | A real boundary: thread panes without his logins and with limited network | A large new piece of work before v1; logins, tools and Docker must work under another token or user |
| Approval prompts instead of yolo | He sees each risky command | Threads stop for him constantly, against parallel unattended work |

## Recommendation
Patch the guard now, OS isolation after v1.

## Choice
- `config/guard.yml` adds deny or ask-first rules for each bypass found and for token exfiltration (`gh auth token`, `az account get-access-token`, reading omp's or Jocasta's auth and credential files), checked against omp's matcher with the test corpus.
- What patterns can't stop (scripts, interpreters, `eval`, anything run as him) is written down as an accepted risk in the README and the thesis.
- OS isolation for thread panes (restricted token, separate user or a container, without his logins and with limited network) waits until after v1 (`docs/status.md` open items).

## Reason
"yes the recomended, the idea is to give jocasta the freedom but also to stop the most dangerous stuff just like in the comics".

## Consequences
- Amends 0450 (the list adds the bypass and token-exfiltration rules; its known limits become an accepted risk in the README and the thesis).
- `docs/status.md`'s open items gain OS isolation for thread panes, after v1.
- 0482's H1 measures the guard against its corpus; 0481 re-tests the rules.
