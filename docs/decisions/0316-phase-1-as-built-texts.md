# 0316 Phase 1 as-built texts and tables

- Status: decided
- Kind: product
- Phase/slice: 1/1.1–1.8
- Date: 2026-10-08

## Question
Are the builders' visible choices accepted as built?

## Facts
- No record set these; they were built minimally and shown for confirmation (tick-list, 0089).

## Options
| Option | Pros | Cons |
|---|---|---|
| CLI output | — | — |
| Thread titles | — | — |
| Hashline grammar | — | — |
| Guard messages | — | — |
| Branch offer text | — | — |
| `catalog.db` v1 tables | — | — |

## Recommendation
All as built.

## Choice
All six as built:
- CLI output: `project list` → `<id>  <name>  <main path>`; `thread list` → `<id>  <kind>  <state>  <branch|->  <title>`; `thread show` → `key: value` lines; errors `jarvis: <message>`.
- Thread titles: the home thread is "Home"; a project thread takes the project's name.
- Hashline: `read` shows `N#tttt: text` (4-hex xxHash32 of the line without trailing whitespace); `edit` takes `REPLACE N#tttt[..M#tttt]`, `INSERT AFTER N#tttt` (`0` = top) and `DELETE N#tttt[..M#tttt]` with `+` body lines; a stale tag writes nothing and shows the current lines.
- Guard messages: `blocked: Use the <tool> tool instead.`; `Loop guard: stopped after 5 identical <tool> calls in a row.` and `Loop guard: stopped after 3 malformed calls in a row.`
- Branch offer: `Branch X has no workspace thread. Start one?`, then `Title:` and `Brief:` prompts, then `[start] [open project thread (chat only)] (s/o)`.
- `catalog.db` v1: `projects(id, identity UNIQUE, created_at)`, `project_paths(path, project_id, role main|worktree|clone, git_config_mtime, added_at)`, and home threads in `threads`.

## Reason
Not given.

## Consequences
- Appendix I's migration ledger names the v1 tables.
