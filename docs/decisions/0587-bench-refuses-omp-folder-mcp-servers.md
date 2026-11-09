# 0587 The task benchmark refuses an omp folder that gives its omp arms MCP servers

- Status: v1
- Kind: benchmark
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
After the review's fixes, an aggregated advisory review asked whether the task benchmark's omp arms load the MCP servers listed in the omp folder they run on. They do. How does the benchmark keep them out?

## Facts
Verified 2026-11-09 in omp 18.18.2's installed source and on his PC.
- omp starts the servers in its agent folder's `mcp.json` and `.mcp.json` in every session (`discovery/builtin.ts:234-242`). That folder is the profile's with `--profile` (`~/.omp/profiles/<name>/agent`, `pi-utils` `dirs.ts:339`), else `PI_CODING_AGENT_DIR`, else `~/.omp/agent`. No setting turns the folder's servers off for one run (`mcp/settings.ts` has `mcp.enableProjectConfig` for project files only), and omp has no command that lists them.
- `jocasta setup` (migration 1) writes `jocasta` and, when whiteboard is built, `whiteboard` there (`src/setup/setup.ts:454-493`). The bench passes no `--profile` unless given one, and trials inherit its environment (`evals/runner.ts:568`).
- A layered arm's own server is also named `jocasta`, in its extension root's `.mcp.json` (`src/frontier/layer/bench.ts:164-177`). omp keeps the first item of a name by provider priority (`capability/index.ts:285`), and the native folder (100) outranks extension packages "so an extension package never shadows the user's own `.omp/` configuration" (`discovery/omp-plugins.ts:12-14`). An entry turned off still claims its name (`capability/index.ts:264-269`, `mcp/config.ts:141-144`), and `disabledServers` hides a name from every source.
- So with setup's entries in the folder, A0–A3 get `graph_search` and whiteboard's tools, and A6's trials reach the installed `jocasta mcp` without the trial's data or `JOCASTA_THREAD`, losing `report_result` and `check`.
- In the VM, `fullrun-prep.ps1` moves setup's files out of tester's omp folder before the full run and the run's script refuses with them there (0508, 0530); the sandbox's runs need them back (`restore-setup-files.ps1`). On his PC the folder lists `jarvis` and `whiteboard`. omp's JSON stream doesn't list a session's tools, so the pilot's transcripts can't show which servers its trials had; none of its A0 trials called one.

## Options
| Option | Pros | Cons |
|---|---|---|
| The bench refuses to start | The arms stay as decided; catches leftovers from setup or H9 whoever runs it; no new sign-in | One cleanup step when it refuses; reads only omp's own files, not servers omp imports from other tools' configs |
| A dedicated `--profile bench`, plus the same check | Also keeps the run's sessions and settings apart | A sign-in to both accounts inside the profile; the run command changes |
| An operator step in `bench/README.md` only | Nothing to build | Easy to miss; a miss breaks A6 and skews A0–A3 for the whole run |

## Recommendation
The bench refuses to start.

## Choice
Before any trial, and in `--dry-run` (not `--validate`, which runs no arm), a run with an omp arm reads `mcp.json` and `.mcp.json` in the omp folder it uses (the `--profile`'s when given) as omp reads them, and stops with exit 2 while a server there is on (`enabled` not false or named in `enabledServers`, and not named in `disabledServers`), naming each file and server: `bench: the omp arms would load MCP servers from <file>: <names>. Remove them (jocasta uninstall removes Jocasta's) or use a --profile without them; no trial ran.` With a layered arm in the run, a `jocasta` entry, even one turned off, or `jocasta` in `disabledServers` stops it too: `bench: <arms> would lose the layer's own MCP server "jocasta": <file> has a "jocasta" entry, which hides the layer's even turned off. Remove it or use a --profile without it; no trial ran.`

## Reason
He chose "Refuse to start".

## Consequences
- Amends 0508 (the VM run script's check for setup's files becomes the bench's own, for any run and operator) and 0543 (A0–A3 and A6 run with no MCP server from omp's folder).
- `ompAgentDir` (`src/shared/paths.ts`) names a profile's folder and follows `PI_CONFIG_DIR`, as omp does.
- No result-format change. Servers omp imports from other tools' configs (Claude Code, Cursor, VS Code and others) aren't checked.
