# 0446 Finishing v1: sandbox until agreed, then remove everything installed

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-15

## Question
How is v1 finished on his machine, and what is removed before he pushes and installs it himself?

## Facts
Verified 2026-10-15.
- Jarvis is installed in place at `%LOCALAPPDATA%\Programs\jarvis` (0440), with its PATH entry, autostart value, data in `%LOCALAPPDATA%\jarvis` (including about 1 GB of speech models), config in `~/.jarvis/agent`, setup's writes into omp, herdr and Windows Terminal, the calendar credential, and Docker containers, images and volumes.
- Live checks already run in their own herdr session and folders (0441).

## Options
| Option | Pros | Cons |
|---|---|---|
| Install each slice in place | He uses every change at once | His install changes under him |
| Sandbox until agreed | His install stays put; checks stay isolated | He sees changes only in the checks |

## Recommendation
Install each slice in place.

## Choice
Until he agrees v1 is done, changes are built and checked in a sandbox (0441), never installed in place. Then everything Jarvis installed is removed: the install (the folder, its PATH entry, the autostart value, herdr session `jarvis`); the containers, their images and every Jarvis volume; the data and memory (`%LOCALAPPDATA%\jarvis` with the models, the Graphiti volume `jarvis_falkordb-data`); Jarvis's config and setup's writes (`~/.jarvis/agent`, omp's MCP servers `jarvis` and `whiteboard`, herdr's omp extension, the Windows Terminal scheme, the calendar credential), keeping his omp `config.yml`. He then pushes to GitHub and installs it himself.

## Reason
"you can use sandbox for working on it but i will install it globally after we agree on v1"; all four removals ticked.

## Consequences
- Differs from the recommendation: no in-place install of this slice.
- Ends 0440 (6)'s in-place installs.
- Amended by 0468: the cleanup runs through `jarvis uninstall`, which undoes exactly what install and setup added.
- Amended by 0487: after the 0481 campaign passes, the agent removes Jarvis by this list (keeping `private-names.txt`, moved to `~/.jocasta/agent/`, and mise) and installs Jocasta from the local build.
