# 0595 The cutover now: Jarvis is Jocasta, everything from Jarvis is deleted

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-10

## Question
He asked to install Jocasta, start using it, and develop Jocasta inside Jocasta. 0487 put the cutover after the 0481 campaign and his v1 review. When does the old Jarvis go, and what happens to its data and its containers?

## Facts
Verified 2026-11-10.
- His PC runs Jarvis 0.1.0 from before the rename, in `%LOCALAPPDATA%\Programs\jarvis` (0.55 GB). It predates 0468 and has no `uninstall` command, so 0446's list runs by hand:
  - the folder, its user PATH entry and the logon Run value `Jarvis`;
  - the Windows Terminal fragment `jarvis`;
  - omp's MCP servers `jarvis` and `whiteboard`, and herdr's omp integration;
  - `~/.jarvis/agent`.
- No calendar credential is stored, and herdr isn't running, so there is no herdr session `jarvis`.
- Its data folder `%LOCALAPPDATA%\jarvis` holds 1.46 GB: speech models (1.07 GB), voice (348 MB), the four eval runs `docs/status.md` named as the omp baseline (26 MB), and small memory, catalog and whiteboard files. Its project list names one project; the Jocasta repo isn't on it.
- Its compose project `jarvis` publishes Graphiti on 127.0.0.1:47380, the port Jocasta's own project uses (`GRAPHITI_PORT`). Both pin the same FalkorDB image digest.
- The omp session he works in (18.18.2) keeps Jarvis's `jarvis.exe mcp` and whiteboard's `node.exe` running from Jarvis's folder. omp 18.18.6 can go on only once every omp is closed (0589).

## Options
| Option | Pros | Cons |
|---|---|---|
| Cutover now | One product: no two `graph_search` tools in every omp, no clash over `whiteboard` or port 47380; the week of use (0573) tests what the weekend measures | No Jarvis to go back to; plain omp still works |
| Jocasta beside Jarvis for the week | Jarvis stays as a fallback until his v1 review (0487) | Two `graph_search` tools, one Graphiti at a time, and Jarvis's later removal by hand around Jocasta's files |

If Jarvis goes now:
- its data folder: keep it for now, keep the evals and remove the rest, or remove it all;
- its containers: remove them and keep the graph's volume, remove everything, or keep everything.

## Recommendation
Cutover now; keep the data folder for now; remove the containers and keep the graph's volume.

## Choice
- The cutover runs now, before the benchmarks and his v1 review: Jarvis is Jocasta, and everything from Jarvis is deleted.
- 0446's list runs by hand, and its data folder goes whole, with the eval runs.
- The compose project `jarvis` goes whole: its containers, its `jarvis/graphiti` image and its volumes, the old graph with them.
- `private-names.txt` moves to `~/.jocasta/agent/`, and mise stays (0487).
- The agent installs Jocasta from the local build with `JOCASTA_INSTALL_FROM` and runs `jocasta setup` with him answering each diff (0487). The repo becomes a project, and Jocasta's own development continues inside Jocasta (0487).
- omp 18.18.6 goes on when he closes his omp (0589).

## Reason
His answers: "jarvis is jocasta now, everything from jarvis is deleted"; "Remove it all" for the data folder; "Remove everything" for the containers.

## Consequences
- Amends 0446 and 0487: the cutover runs before the 0481 campaign ends and before he agrees v1 is done.
- Amends 0468: the installed Jarvis has no `jarvis uninstall`, so 0446's list ran by hand.
- The data folder and the graph differ from the recommendation.
- Builder's choices, listed as built (0363):
  - FalkorDB's pinned image stays, since Jocasta's project uses the same one and would download it again;
  - herdr's omp integration is removed so setup installs it as Jocasta's;
  - the agent's own omp session stops its two Jarvis servers, so Jarvis's folder can go.
- The private-names scan reads `~/.jocasta/agent/private-names.txt` (AGENTS.md).
- The eval baseline runs named in `docs/status.md` are gone; the evals' next run sets a new baseline.
- His week of use (0573) runs on the installed Jocasta.
