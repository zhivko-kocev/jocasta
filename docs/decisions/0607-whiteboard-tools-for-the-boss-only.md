# 0607 Whiteboard's tools are mounted for the boss only

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V6
- Date: 2026-11-14

## Question
Setup writes the `whiteboard` MCP entry into the user-level `mcp.json`, so every project pane, thread and subagent mounts whiteboard's tools. Who gets them?

## Facts
Verified 2026-11-14 (`docs/research/state-of-the-art.md` section 5).
- Setup's migration 1 writes `jocasta` and `whiteboard` into `~/.omp/agent/mcp.json` (`src/setup/setup.ts`, `globalServers`). Migration 2 writes the boss's own `mcp.json` under `<data>/home/.omp/` (`bossServers`).
- Every omp session that mounts the entry starts a Node process (`whiteboard mcp`).
- Its fixed prompt cost depends on the start order. A session that starts before whiteboard's server is reachable lists 2 tools, about 137 tokens estimated from bytes: 537 of 543 sampled sessions. One that starts after lists 23, about 1,300. A pane's whole fixed prompt is about 11.4k.
- 322 of November's 386 sessions mounted the entry (83%).
- Whiteboard's tools were called 41 times in all, in 3 of 3,287 session files, and not once after 2026-10-14.
- The shipped texts that tell an agent to draw are the boss's: the manual's `show` and the boss skill `whiteboard-review`. No thread or project-pane text asks for a drawing.
- V6 keeps diagrams and animations. This record does not touch what the boss can draw.

## Options
| Option | Pros | Cons |
|---|---|---|
| The boss only | Every pane and subagent drops the text and the process; about 15 lines | A project pane or thread can no longer draw by itself: he asks the boss, which has to read the change first; installs need a migration |
| A skill and `whiteboard api` for every pane | Every agent can still draw, at about 50 fixed tokens and no process | Not verified on Windows (JSON quoting; whether the command's schema equals the MCP one); a spike first |
| Leave it (0393 as decided) | Any agent can draw | Every session keeps paying for a tool three sessions called |

## Recommendation
The boss only.

## Choice
The boss only.
- `globalServers` no longer carries `whiteboard`; `bossServers` does.
- A new setup migration, id 17, removes the `whiteboard` entry from the user-level `mcp.json` when it is still the one Jocasta wrote. An entry he changed is left and named.
- `jocasta uninstall` removes the boss's entry with the boss's file, as it does today.

## Reason
His answer: "boss-only".

## Consequences
- Amends 0393 ("agents draw through `whiteboard mcp`": now the boss does) and 0468's list of servers uninstall owns in the user-level file.
- His plain omp sessions read the same user-level file, so they lose whiteboard's tools too.
- Plan appendix I gains migration 17 (0027).
- Before: section 5's estimates. After: the tool lines a thread pane lists, read from a new session.
