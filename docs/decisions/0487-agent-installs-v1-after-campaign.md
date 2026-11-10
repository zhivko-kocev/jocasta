# 0487 The cutover: the agent installs v1 after the campaign passes

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-24

## Question
Who removes the old Jarvis and installs Jocasta on his machine, and when?

## Facts
Verified 2026-10-24.
- 0446 removes everything Jarvis installed once he agrees v1 is done, through `jarvis uninstall` (0468), keeping his omp `config.yml`.
- 0480 renames the product without migration code, so his machine is cleaned and installed fresh.
- The installers download from GitHub releases; agents never push (0003), so no Jocasta release exists until he pushes.

## Options
| Option | Pros | Cons |
|---|---|---|
| I do it after the campaign passes | The install he uses is the one the campaign passed; he only answers setup's diffs | Waits for the campaign |
| You do it yourself after you push | The real `irm … \| iex` path is used at once | His time; the local build's install isn't tried on his machine |
| Now, before the campaign | He uses Jocasta at once | His daily install changes under him while it is still being tested (against 0446) |

## Recommendation
I do it after the campaign passes.

## Choice
- After the 0481 campaign passes and he agrees v1 is done, the agent removes the old Jarvis by 0446's list:
  - `private-names.txt` is kept and moved to `~/.jocasta/agent/`;
  - mise stays (0476).
- The agent then installs Jocasta through `scripts/install.ps1` from the local build, with `JOCASTA_INSTALL_FROM=<folder holding jocasta-windows-x64.zip and its .sha256>` (also used by the 0481 VM run).
- The agent runs `jocasta setup` with him answering each diff. Jocasta's own development then continues inside Jocasta: the repo is a project, and threads work on `<repo>.wt/<slug>`.
- The `irm … | iex` path from GitHub is tested once he pushes a pre-release (agents never push).

## Reason
"I do it after the campaign passes".

## Consequences
- Amends 0446 (the agent, not he, installs v1 from the local build after the campaign; `private-names.txt` and mise stay).
- Plan appendix I names `JOCASTA_INSTALL_FROM`.
- Amended by 0595: the cutover runs on 2026-11-10, before the campaign ends and before he agrees v1 is done; everything from Jarvis is deleted, its data folder and graph included.
