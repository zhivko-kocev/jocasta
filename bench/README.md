# Benchmarks (0482)

The harness of record [0482](../docs/decisions/0482-benchmarks-pre-registered.md), the thesis's pre-registration: what
runs, how often and what is measured is fixed there, as amended for the harness-OS rerun by 0533, 0537, 0538, 0543,
0545, 0546 and 0552. A change to the procedure before the run is a new record.

- Fixtures and raw results live outside the repo, in `D:/thesis/data/`. Every script takes `--out <dir>`; a folder
  inside the repo is refused.
- Every script takes `--runs <n>` (its default is the pre-registered count) and `--dry-run`, which prints what it would
  start, kill and measure, and runs nothing.
- Each writes `<out>/<name>.json` (the summary, with the run's conditions: time and local hour, OS, CPU, memory and the
  tools' versions, no host or user names; the scripts that measure Jocasta also carry `jocasta --version`'s line and
  `jocastaSha256`, the SHA-256 of the `jocasta` on PATH, so each row names the build it measured, 0533, 0576) and,
  where it has them, raw rows in `<out>/<name>.jsonl`. A script refuses an `--out` that already holds its `.jsonl`
  ("results are never overwritten"), so a rerun never mixes rows of two runs; tokscale parity rewrites its own rows.
- Medians, p95 and percentile-bootstrap 95% CIs (10,000 resamples, seed 482) come from `lib/stats.ts`.
- Cost is priced with one table for every arm and benchmark (0538): LiteLLM's `model_prices_and_context_window.json`,
  fetched once and committed in `prices/` with `prices/source.json` (its URL at commit `e58354f0…398f` of
  2026-11-08, fetched 2026-11-09, and its SHA-256, which `lib/prices.ts` checks on every load). Cache read and cache
  write are priced apart, a 1-hour cache write at its own rate. The ledger (`lib/ledger.ts` on the product's
  `src/shared/ledger.ts`, which owns the rows, waste and peak) reads omp's session files,
  Claude Code's transcripts and opencode's `opencode export` JSON into rows per model request (time, model, uncached
  input, cache read, cache write, output, cost) and per tool call (tool, argument hash, file path, result size,
  error). In omp's session files a request is every assistant message and every side call omp records as
  `model_usage` (cache warming, compaction and the like). A session's copies (a forked or moved session's files, a
  subagent transcript copied along) count once. Per session it counts waste (re-reads of an unchanged file, the same
  call repeated, results of 50,000 bytes or more, repeated failures of one tool; cache misses: a cache write of 10,000
  tokens or more after more than 5 minutes without a request on the same provider and model; compactions: omp's own,
  `soft`, `shake`, `remote` and legacy, not Jocasta's `/handoff`, `method: "handoff"`, 0582) and the peak context
  (tokens and percent of the model's window from the table). The service's own model calls (Graphiti's extraction
  through its model endpoint, logged in `<data>/graph-usage.jsonl`) aren't in the ledger.

## Where they run

| Script | Model calls | Where |
|---|---|---|
| `h1-guard/omp-verdicts.ts`, `h1-guard/evaluate.ts` | none | anywhere with omp 18.18.6 installed (no command runs) |
| `h1-guard/live-check.ts` | 31 cheap-model turns (30 picks and a canary; a few cents) | 0481's VM (an ACP client records omp's calls, 0585) |
| `h1-guard/git-guard-check.ts` | none, but it runs the corpus's git lines (`--run`) | the VM `JOCASTA-CLEAN` only |
| `h7-longpath.ts` | none | anywhere (git only, temp folders) |
| `h2-toast.ts`, `h3-recovery.ts`, `h5-startup.ts` | none, except `h2 --start` | a sandbox Jocasta |
| `h4-isolation.ts` | the `h4 plant` thread's turn and Graphiti's extraction of the planted facts | his PC, in a sandbox Jocasta with Docker (0502) |
| `h6-memory.ts`, `h8-parallel.ts` | the threads' turns | 0481's VM |
| `h9-footprint.ts` | none | 0481's VM `JOCASTA-CLEAN` only (refused elsewhere) |
| `tokscale-parity.ts` | none | the VM, with tokscale 4.21.0 on PATH |

The scripts that act on a running Jocasta (H2–H6, H8) refuse to start without a sandbox (0502, 0576): its own data
folder in `JOCASTA_DATA_DIR` (not the OS default), a herdr session in `JOCASTA_HERDR_SESSION` that isn't `jocasta` (or
`jarvis`), and its own config folder in `JOCASTA_CODING_AGENT_DIR` (not `~/.jocasta/agent`: the config folder picks the
projects, herdr's config and the Graphiti compose project, so a sandbox on his would recreate his Graphiti container
with its own token). Start the sandbox's `jocasta` with the same three. H2, H4 (its `h4 plant` thread), H6 and H8
start their threads as `jocasta thread new` does (`POST /api/cli`: no Allow, no boss key, 0544) and read them as
`jocasta thread list` does;
threads still open in plan mode and wait for the operator's approval of their plan in their pane. The bench never
closes threads or removes worktrees: those ask him first (0484), so H6 and H8 wait for Enter between runs.

First of all, the VM's omp moves to 18.18.6 (0589), the way 0497 installed it:
`bun install -g @oh-my-pi/pi-coding-agent@18.18.6`; then `omp --version` must print 18.18.6. The release rebuilt for
this commit goes there too, so every row runs on that omp and that build.

Before the full runs (0588), each script runs once in the VM's sandbox, each into its own throwaway `--out` (checks,
not results): every script's `--dry-run`, then `h3-recovery.ts --runs 1`, `h5-startup.ts --runs 1`,
`h6-memory.ts --threads 0,1` and `h8-parallel.ts --threads 2`; anything broken is fixed before the long runs. These
runs need setup's files in tester's omp folder (`restore-setup-files.ps1`); the task benchmark needs them out
(`fullrun-prep.ps1`) and refuses to start while that folder gives its omp arms an MCP server (0587).

## The scripts

- **H1 (0482, 0500, 0501, 0535, 0589).** Four scripts; the corpus is a separate file and this folder holds no command list.
  H1 runs on the build's omp, 18.18.6 (0535, 0589); `--any-version` is the exploration override.
  - `bun bench/h1-guard/omp-verdicts.ts --corpus <corpus.jsonl> --guard <guard.yml> --out <dir>
    [--git-guard <git-guard-check.jsonl>] [--omp <package dir>] [--any-version]` imports omp's own
    `config/settings.ts`, `tools/bash.ts` and `tools/approval.ts` from the install (found from `omp` on PATH or Bun's
    global install; anything but 18.18.6 is refused), loads the guard as `--config` over omp's defaults, builds the
    real `BashTool` on a session stub that throws if execution is reached, and asks omp's `resolveApproval` for each
    line, as a pane's tool wrapper does. Nothing runs. omp's git guard refuses only when the embedded shell runs `git`:
    with `--git-guard` the refusals `git-guard-check.ts` measured decide for the lines it ran, and the
    reimplementation (`git-guard.ts`) for the rest; each line's `gitGuardSource` says which. A line omp allows and the
    git guard refuses is `deny` by `git-guard`. Writes `omp-verdicts.jsonl` (verdict, mechanism, the guard pattern,
    omp's approval alone, whether omp's critical list flagged it, the git guard's source, and the reimplementation's
    verdict) and prints where they disagree, by expected class and way of writing.
  - `bun bench/h1-guard/git-guard-check.ts --corpus <corpus.jsonl> --out <dir> (--dry-run | --run)` measures the git
    guard. It takes every line made only of git (any spelling the corpus uses, also inside `pwsh -c`/`cmd /c`), `cd`
    and `echo` (423 lines), and runs each twice through omp's own `executeBash` in `C:\work\demo`, with
    `gitGuard: true` and then `false` as the control, each on a fresh fixture: commits on `main`, the branches the
    corpus names with `feature/x` holding its own commit, a tag, a bare `origin` with everything pushed, a stash entry,
    `src/app.ts` modified and an untracked file, with git's config isolated in a temp folder. It refuses without
    `--run`, off the VM `JOCASTA-CLEAN`, or when `C:\work` holds more than the fixture. Writes
    `git-guard-check.jsonl` and `git-guard-check.json` (agreement with `git-guard.ts` per subcommand and way of writing,
    every disagreement listed).
  - `bun bench/h1-guard/evaluate.ts --corpus <corpus.jsonl> [--guard <guard.yml>] [--verdicts <omp-verdicts.jsonl>]
    [--out <dir>]` scores, per group and per way of writing (0501: the collected commands as written, and each
    rewrite), deny recall, must-deny asked, must-ask stopped, false deny, false ask and ask rate, each with a CI. The
    as-written row is the 224 lines whose command is one of the collected commands as written: only 223 carry the
    `as-written` tag, because the builder keeps one line per command and `rm -rf node_modules` (h1-1115) is also
    `rm -Rf node_modules` lowercased, so it sits under `lowercase-powershell` and counts in both rows (`notes` in
    `h1-guard.json` says so). With `--verdicts` it scores omp's verdicts (the result of record); without, its own
    reimplementation (`guard.yml`'s rules only, raw-text segments), kept for the unit tests and as the comparison
    column. `--guard` defaults to `config/guard.yml`; the frozen pre-0485 guard is `bench/h1-guard/guard-pre-0485.yml`,
    and the guard as H1 measured it after 0485 is `bench/h1-guard/guard-0485.yml`. Writes `h1-guard.json` (with the
    corpus's SHA-256, the matcher and the as-written note) and one verdict per line in `h1-guard.jsonl`. The
    reimplementation's results of 2026-10-24 are in
    `D:/thesis/data/bench/h1/{pre-0485,0485,campaign}/`, omp 18.13.1's in `D:/thesis/data/bench/h1/omp-18.13.1/`;
    the build's runs on omp 18.18.6 go in a folder of their own (`D:/thesis/data/bench/h1/omp-18.18.6/`). omp
    18.18.1 changed how its pattern rules combine (a deny matching any part of a compound line beats any prompt;
    critical patterns come before prompt rules), so `omp-verdicts.ts` is run again on the build's omp, 18.18.6 (on the
    0580 corpus its verdicts matched 18.18.2's line for line before the move, 0589); and 18.14.0 rewrote
    the git guard (whose checkout git is in first; refusals for a failed save of uncommitted changes and for
    submodule recursion), so `git-guard-check.ts --run` is run again in the VM before H1 reports measured refusals.
  - **The 0580 lines.** The guard's 0580 rules (combined force flags, trunk pushes, restore, update-ref and git's
    credentials, the graph's containers, omp's credentials and critical shapes, `gh repo edit --visibility`, the
    `*GIT*`/`*Git*` copies, merge and publish asks) each have labelled lines in the corpus's `dev` group, taken from
    `test/config/guard.test.ts` (the guard's author saw them, so they are `dev`, never held-out):
    `D:/thesis/data/bench/h1-dev-0580.jsonl` (206 lines; 201 after the builder's duplicates), built with
    `build-corpus.ts --dev` into `D:/thesis/data/bench/h1-corpus-0580.jsonl` (1,486 lines, SHA-256 `37d77a8f…0718516`).
    Its 1,285 held-out lines are the frozen corpus's, byte for byte (`276b223a…39b1de32`, `corpus.sha256`).
  - `bun bench/h1-guard/live-check.ts --corpus <corpus.jsonl> --out <dir> [--guard <guard.yml>] [--verdicts
    <omp-verdicts.jsonl>] [--model anthropic/claude-haiku-5-5] [--sample 30] [--seed 482]` is the design's check of the
    in-process verdicts against real omp: 30 random lines (seed 482), each one omp turn told to run that command once,
    read back as refused, asked or ran. The prompt shows the command verbatim between two marker lines (shown
    JSON-escaped, the model copied the escapes). A pick compares only when the call carries the command unchanged; the
    model may decline a destructive one (no-call), which leaves it out. **The recorder (0585)** is an ACP client
    (`acp-recorder.ts`): each turn is `omp --mode acp` with the script as its client, which advertises a terminal, so
    every bash call omp's approval lets through comes to the client's `terminal/create` (recorded as ran; the
    terminal runs nothing and answers with a marker), a prompt verdict comes as `session/request_permission` (asked;
    the client rejects it once), and a deny ends the call with omp's policy text (refused). The overlay (a second
    `--config` after the guard) sets `tools.approvalMode: yolo` explicitly, since ACP asks for every bash call under
    the schema's implicit yolo, and turns launch and async off; a recorder extension (`-e`) drops `pty`, `name`,
    `ready` and `async` from bash calls and blocks a URL working folder, the only ways a call would run in omp's own
    process. Before any model call it checks with omp's code that the settings layer so, the interceptor is off, and
    this omp has the client-terminal route and a reject-once permission option; an omp without them is refused. Then
    a canary must reach the client's terminal with no file written. `--dry-run` prints the picks, the model, the
    command line, the overlay and the recorder. Writes `live-check.jsonl` (each row also has the terminal command
    line, the turn's stop reason and any failure) and `live-check.json`. Run it on the frozen corpus (1,285 lines),
    as 0501 sampled it: the 0580 corpus has more lines, so seed 482 picks other lines there. In the VM: `--dry-run`
    first, then the run (a few cents), e.g. `--corpus D:/thesis/data/bench/h1-corpus.jsonl --out
    D:/thesis/data/bench/h1/omp-18.18.6/live`; check that the canary reached the recorder's terminal ("canary reached
    the recorder's terminal … nothing written"), that `live-check.json` names omp 18.18.6 and the recorder `ACP client
    terminal (0585)`, that the outcomes agree with the in-process verdicts, and that no `h1-live-canary.txt` is left.
    The 18.13.1 run (24/24) used omp's bash interceptor as the recorder, which 18.15.0 moved into the bash tool's
    approval.
- **H2, `bun bench/h2-toast.ts --out <dir> [--runs 30] [--condition host|herdr] [--start <project>]`.** Jocasta raises
  its notifications through herdr's `notification.show`, which herdr hands to Windows' notification service through
  the foreground client and answers `shown` (or `disabled`, `rate_limited`, `no_foreground_client`, `busy`). H2
  records herdr's `pane.agent_status_changed` pushes (t1, kept as a reference figure) and reads the service's
  notification log, `<data>/notifications.jsonl` (one line per call, written when herdr's reply comes back: `at`,
  `paneId`, `reason`, `title`, `body`, `result`). t0 is the pane's omp session file's `ask` call or plan proposal; an
  ask counts when omp completed its message (`completedAt`, else the entry's time), not when it started the request,
  so the model's generation is no longer inside t1−t0 and t3−t0 (the 0517/0520 rows include it; their session files
  weren't kept, so they can't be recomputed). **t3 is the moment herdr answered `shown` for the pane's `blocked`
  notification**: the notification reached Windows; how long Windows then takes to draw the banner isn't measured.
  Each row also names herdr's answer to the pane's first `blocked` notification (`notificationResult`), so a trial
  without t3 says why. **Conditions (0576):** `host` (the default): the service reads each thread's state from omp's
  collab host, polled every 1 s (0547); `herdr`: the sandbox service started with `JOCASTA_HOSTS=off`, so it ignores
  omp's host registry and falls back to herdr's events and its tick. H2 reads the switch from `/health` and refuses a
  mismatch. Without `--start`, trials come from threads started by hand (the boss's and the today pane left out).
  `--start` starts each trial as `jocasta thread new` does and counts only those threads' panes; the next starts once
  the last one's notification came, or after 5 minutes without one, which `h2-toast.json`'s `missed` lists. Writes
  `h2-toast.jsonl`, `h2-toast.json` (t1−t0, t3−t0 and the count of each herdr answer) and the run's lines of the log
  in `h2-notifications.jsonl`.
- **H3, `bun bench/h3-recovery.ts --out <dir> [--runs 20] [--faults herdr,service,pane] [--restart jocasta]`.**
  Kills herdr's server, the service, or one omp pane and times recovery from the moment the kill returned (the
  lookups before it; the herdr and pane faults' kill, the product's `killTree`, returns once the process tree is gone,
  about 50 ms after the bench's own kill did before 0573's fixes). Every fault counts as recovered when every
  workspace that had an omp pane before has an omp
  pane again, whatever its pane id (0522: the service relaunches panes under new ids); the service fault also needs a
  new service answering (0528: it relaunches the boss pane under a new id with its new key). Before 0576 the service
  fault was named `companion`; `h3-recovery.json`'s `"renamed": {"service": "companion"}` says so. For the herdr
  fault each row also gives `oldPaneIdsMs`, the time until herdr first listed exactly the old pane ids (null when no
  250 ms poll saw them), summarized beside `recoveryMs`. The pane fault ends the process tree herdr started for the
  omp pane in the folder it killed longest ago, one at work first among equals, and counts it back when omp runs again
  in that workspace under another pane id; it waits, when it must, until 10.5 minutes have passed since that folder's
  last run ended (0528): 0521 restarts an omp that keeps ending in one folder after a wait that doubles from 10 s, so
  closer kills would time that wait, not recovery. After each service run the restart's client ends, root first and
  never the new service's tree, so one hidden client is attached at a time. A run after which Jocasta has closed
  itself (no service answers, one still closing waited for up to a minute, and herdr lists no panes) counts not
  recovered with `stoppedItself` (0530): `--restart` starts Jocasta again, its client left attached in place of the
  terminal's, and the next run waits until a service answers and omp is back everywhere (`restartMs`; past 5 minutes
  H3 ends). Lost work: each pane folder's git status and diff hash, and each session file still parses: every
  non-empty line is JSON and a `session` entry is in it, wherever it is (0523; omp 18.13.1 wrote a `title` entry
  first). The plugin's `today` pane (0553) is left out of every pane check: it is no thread, and the service opens it
  again under a new id. Logging off and on is done by hand with the same checks. Writes `h3-recovery.jsonl` and
  `h3-recovery.json`.
- **H4, `bun bench/h4-isolation.ts --out <dir> --project-a <p-…> --pane-a <pane> --pane-b <pane> [--canaries 50]
  [--runs 100] [--wait 10] [--plant-timeout 30] [--skip-plant]`.** Runs on his PC in a sandbox Jocasta with Docker
  (0502). Plants the canaries through the service (0576): it starts a thread `h4 plant` in A as `jocasta thread new`
  does, whose brief tells it to call `report_result` once with exactly the canaries as decisions (`<topic> codename` =
  the token, why `H4 canary`); the service posts them to A's graph as one "Decisions settled by the thread "h4 plant"
  …" episode with its own Graphiti token and model endpoint (0539). The operator approves the thread's plan in its
  pane; once `jocasta thread list` shows its report (within `--plant-timeout` minutes), H4 waits `--wait` minutes for
  Graphiti's extraction. Then it queries `graph_search` from B's pane through the service's MCP endpoint (as that pane:
  its key from the sandbox's `<data>/panes.json`; a pane without one is refused: `pane <id> has no key in
  <data>/panes.json: pass a pane Jocasta started`) and checks every answered reply for every canary; errored replies
  are counted apart and aren't checks. The same queries from A's pane are the required positive control: the
  rule-of-three bound is given only when it found canaries, else the run is `inconclusive`. Scans every other
  project's `mnemopi.db` and `mnemopi.db-wal` for the canaries. Prints e.g. `0 leaks in 4,900 answered checks (2
  queries errored); positive control found canaries in 92/100 queries; 95% upper bound 3/4,900`. Writes
  `h4-canaries.json` and `h4-isolation.json`.
- **H5, `bun bench/h5-startup.ts --out <dir> [--runs 20] [--tools jocasta,omp,claude,opencode] [--cwd <repo>]
  [--ready tool=<regex>]`.** Cold (after 30 s with the tool's processes gone; an OS-cold start needs a reboot per
  run) and warm starts. Jocasta ends at the same point as omp alone (0576): `readyMs` is the boss pane's screen
  matching omp's ready pattern; each row also has `bossIdleMs` (herdr reporting the boss's omp idle) and `serviceMs`
  (the new service answering `/health`; rows before 0576 call it `companionMs`, and the summary reads either).
  Between runs H5 closes Jocasta as he does (Stop everything through `/api/shutdown`) and waits until
  `jocasta-service.json` is gone and the `jocasta` it started has exited; after 60 s it ends what is left. The others start
  in a tab of a separate bench herdr session (`--pty-session bench-pty`; Bun's PTY is POSIX-only; never his session
  nor the sandbox's) and are timed to their ready pattern. Each starts as a command line runs its name
  (`toolCommand`: `Bun.which`, with npm's `.cmd` shim for an `.exe` replaced by that `.exe`), because herdr 0.9.5
  picks npm's extensionless shim for Git Bash, which Windows can't start. The default patterns (`lib/launch.ts`) were
  each seen in the VM on 2026-10-28: omp's prompt after 3.6 s, unsigned opencode's after 9.8 s cold while the VM was
  busy, and Claude Code's after 3.7 s (its footer reads "shift+tab to cycle" in its default auto mode, "? for
  shortcuts" in the others). Claude Code asks to trust each new folder, which a bench pane can't answer, so with it in
  the tools H5 and H6 need `--cwd` at a folder trusted once (in the VM, `C:\jc\bench-cwd`); a real run without one
  stops before any work. Writes `h5-startup.jsonl` and `h5-startup.json`.
- **H6, `bun bench/h6-memory.ts --out <dir> [--threads 0,1,4,8] [--project <name>] [--tools omp,claude,opencode]
  [--idle 120] [--turn]`.** Working set and private bytes of Jocasta's processes with 0–8 threads: the service's tree,
  herdr's server tree (the panes with their omp and MCP processes) and each attached surface's tree (bare `jocasta`
  and its herdr client, `<data>/surfaces/`), summed and each part apart (`parts`), and each thread pane alone; each
  competitor in a bench pane after idle, and with `--turn` after one prompt. opencode runs unsigned (0506: its
  Anthropic sign-in takes only an API key), so it gets no prompt and `skipped` says why. Writes `h6-memory.jsonl` and
  `h6-memory.json`.
- **H7, `bun bench/h7-longpath.ts --out <dir> [--lengths 100,…,240] [--root <dir>] [--home <dir>]
  [--location name=<template>]`.** Fixture repos whose deepest file has a relative path of L characters, checked out
  with `git worktree add` at Jocasta's `<parent>/<name>.wt/<slug>`, herdr's `<home>/.herdr/worktrees/<name>/<slug>`
  and Claude Code's `<repo>/.claude/worktrees/<slug>` (confirm in the VM), with Git's default settings. vibe-kanban's
  folder is added with `--location` once confirmed. Writes `h7-longpath.jsonl` and `h7-longpath.json` (the largest
  passing L per location).
- **H8, `bun bench/h8-parallel.ts --out <dir> --project <name> --repo <checkout> [--threads 2,4,8] [--layer on|off]
  [--make-fixture]`.** Pairs of threads edit one file or one interface from both sides; reports wrong-thread edits,
  overlaps flagged before merge (`jocasta thread list`), `git merge-tree` conflicts, semantic failures and conflicts
  found only at merge. An interface pair is the "Passes Alone, Fails Together" case (0576): the fixture's
  `test/pair<k>.test.ts` calls `show<k>(makeItem<k>())`, which passes with either side alone and throws with both;
  for each pair that merges cleanly H8 checks the merged tree out in a temp worktree and runs `bun test`, and a
  failure counts as `semanticFailures` and, unless the overlaps view flagged the pair, as found only at merge. A
  thread counts finished when `jocasta thread list` shows it idle or done and its branch has a commit beyond `main`
  (0529), so one that asks instead runs to the 60-minute limit. **K11 (0543):** `--layer on` and `--layer off` run 4
  threads at once, 3 runs each (the defaults with `--layer`), with the sandbox service started with
  `JOCASTA_LAYER=off` for `--layer off` (0576); H8 reads the layer's state from `/health` and refuses a mismatch. Each
  run adds the cost per finished thread (every omp session of each thread since the run started, its planning session
  before "Approve and execute" included, with its subagents' transcripts, a moved session's copies counted once,
  priced by the ledger), the peak working set and private bytes of Jocasta's processes (the service's, herdr's
  server's with the threads, and the attached surfaces' trees; sampled every 10 s) and, layer on, the run's shared
  check hits and index reuse from the service's logs (`readLayerCounters`). Writes `h8-parallel.jsonl` and
  `h8-parallel.json`.
- **tokscale parity (0538), `bun bench/tokscale-parity.ts --out <task-benchmark runDir>`.** The ledger's parser check,
  bench-only: per saved session of the run, the ledger's uncached input, cache read, cache write and output against
  `tokscale models --json --group-by client,session,model` (omp's saved folders passed as `TOKSCALE_EXTRA_DIRS`, in a
  throwaway `TOKSCALE_CONFIG_DIR`; Claude Code's and opencode's records where they live in the VM). Tokens must match
  exactly; dollars aren't compared. tokscale is pinned at v4.21.0 (2026-11-02, MIT), the Windows binary from
  `https://github.com/junhoyeo/tokscale/releases/tag/v4.21.0`, approved for the VM only and not installed on his PC:
  without it the script stops and `test/bench/tokscale-parity.test.ts`'s live case skips, saying so. Writes
  `tokscale-parity.jsonl` (rewritten on each check) and `tokscale-parity.json`; exits 1 on a mismatch and 2 when it
  compared nothing (`bench: no saved sessions to compare in <runDir>\sessions (results.jsonl names <n> trials)`).
- **H9, `bun bench/h9-footprint.ts --out <dir> --install "<command>" --uninstall "<command>" [--between "<command>"]
  [--roots <dir,…>]`.** Runs only on the VM `JOCASTA-CLEAN` and refuses anywhere else before anything runs.
  Snapshots files (the default roots include `%USERPROFILE%\.config`, where setup writes its mise files, 0468) and the
  registry before install, after install, after `--between` (0481's setup, first run and a thread, when given) and
  after uninstall; reports what install added, what setup and use added, and what uninstall left (all, and what names
  jocasta, herdr, omp or mise as a word). Prerequisites and minutes to the first task go in the run's notes by hand.
  Writes `h9-footprint.json`, `h9-install-added.jsonl`, `h9-use-added.jsonl` and `h9-uninstall-left.jsonl`;
  `h9-footprint.json`'s conditions carry the installed `jocasta.exe`'s `--version` line and `jocastaSha256`, read after
  the first install (from the install folder when this process's PATH predates the install's PATH entry, 0576).

## The task benchmark

<!-- EVL -->

**`bun evals/bench.ts --tasks <tasks.jsonl> --cache <dir> --out <runDir> [--study main|followup]
[--arms a0,a1,a2,a3,a4,a6,a7] [--reps 3] [--set main] [--only id,id] [--pilot] [--workers 2] [--seed 482]
[--timeout 90] [--profile <omp profile>] [--work <dir>] [--pty-session bench-pty] [--validate] [--dry-run]`** runs the 40 curated tasks of
`D:/thesis/data/bench/tasks/tasks.jsonl` through the eval runner's `runTrial` (same transcripts, usage and cost, line
counts, findings and clean-clone hidden grading as `bun run evals`). The main run (0543) is 30 tasks × 3 reps × 7 arms
= 630 trials: A0 omp alone, A1 + the pane config, A2 + plan first, A3 + the review, A4 Claude Code, **A6** = A3 + the
harness-OS layer, **A7** = opencode bare. Results go to `D:/thesis/data/bench/tasks/runs/<run>/`: `results.jsonl`
(the runner's row plus `task`, `arm`, `language`, `attempt`, `trialIndex`, `leak` and `ledger`, the trial's ledger
totals: tokens, priced cost, cache read share, waste counts, peak context; a layered trial's row also has
`layer.indexBuildMs`, and a row in whose output or exports A7's key showed has `keyExposed: true`, the key replaced
there by `<A7 key>`), `ledger.jsonl` (every counted trial's
request and tool-call rows), `spend.jsonl` (each attempt's priced cost, infra errors included), `sessions/<task>-<arm>-<rep>/`
(the harness's own records: omp's session files, written with `--session-dir` in the trial root; Claude Code's
transcript and subagent transcripts, found by the stream's `session_id` under `~/.claude/projects` or
`CLAUDE_CONFIG_DIR`; opencode's `opencode export <session>` of each session its stream names),
`transcripts/<task>-<arm>-<rep>.jsonl`, `logs/<task>-<arm>-<rep>/setup.log` and `grade.log` (each command, its folder,
exit code, seconds, stdout and stderr; a retried trial's attempts append) and `lines.log` (the per-file
`git diff --numstat` behind the row's line counts, 0508), `errors.jsonl` (infra errors), `conditions.json` (seed, trial
order, the conditions the run holds: the trial limit, the profile and each arm's harness version at its start, and per
session start, end, OS, CPU, RAM, the versions of omp, claude, opencode, git, bun, node, python and go, the trial
and grading limits, the work root, git's `core.longpaths`, Windows' `LongPathsEnabled`, the pacer's pauses and A7's
priced spend) and, with `--validate`, `validation.jsonl` with each task's `validation/<task>/setup.log`, `null.log`
and `gold-1.log`…`gold-3.log`. While a bench runs, `<runDir>/run.lock` holds its pid and start: a second bench in the
same `--out` is refused (`<runDir> is in use by a bench (pid <pid>, since <time>); stop it or use another --out`), and
a lock whose process is gone is taken over (0574). Real trials need `JOCASTA_EVALS_REAL=1`; `--dry-run` and
`--validate` call no model. Exits 1 when a trial or task is left failing or an arm stopped on a new harness version,
2 on a bad flag or a refusal (one line, `bench: …`), 3 after three sign-in failures in a row, 4 when A7's spend limit
stopped its trials. The fixed choices:

- **A6 (0543, 0567):** `a6.yaml` extends A3 with `layer` (the code index switch; the `check` thread tool is always on, 0571), and plans
  first as Jocasta's threads do: not A3's `--plan-yolo`, but omp's interactive plan review answered "Approve and
  execute" (`plan_review: execute`); `modelRoles.default` and `.plan` are pinned to `anthropic/claude-opus-5-5:high`, so
  the review's planning role and execution tier stay on the arms' model. The layer is Jocasta's build, not the
  bench's: each A6 trial imports `src/frontier/layer/bench.ts` (`LAYER_MODULE` in `evals/layer.ts`), whose
  `benchLayer.setUp(trial)` returns the omp flags, overlay settings and environment that bring in the task file's brief,
  `report_result`, the cached checks and the index, and a `close()`. The layer's data (its checks, code index and
  endpoint file) goes in a folder of its own per trial, `%TEMP%\jl…`, inside the user's profile, removed at the
  trial's end: codebase-memory-mcp refuses a folder other users may write to, such as the VM's short `--work` root,
  so the trial roots stay in `--work` and only the layer's data moves (EVL-1). Before any trial a real run checks
  each layered arm's code index (`preflight`) and refuses the run when it can't run (`<arm>: A6's code index can't
  run: …`, exit 2); the dry run marks such an arm `NOT RUNNABLE`. A checkout without the layer build is refused the
  same way ("A6 needs Jocasta's layer build"). **The index before the clock (0575):** with the index on, each
  trial's layer builds the code index of the trial's checkout during setup, before the trial's clock; the row records
  the build time apart (`"layer": {"indexBuildMs": 21840}`), and `wallMs` doesn't include it.
- **The plan review (0567, `evals/plan-review.ts`):** omp 18.18.6 offers "Approve and execute" (the session is cleared
  before the plan-approved prompt) and "Approve and keep context" only in its interactive plan review; print mode has
  only `--plan-yolo` and ACP's approval, both keeping the planning context. The interactive review emits
  `plan_review_requested` to extensions, and `ctx.resolvePlanReview(reviewId, "execute" | "keep")` answers it as if he
  had picked it. So a `plan_review` arm's first turn runs `omp <flags> --session-dir <root>/sessions --config <overlay>
  -e <root>/plan-review.js <prompt>` interactive (it needs a TTY) in a tab of the bench herdr session `--pty-session`
  (workspace `bench-trials`), its overlay adding `plan.defaultOnStartup: true`; the per-trial extension answers the
  review, logs to `<root>/plan-review.jsonl`, and shuts omp down at the end of the approved plan's execution (or when a
  planning turn ends without a proposal). The turn counts as exited well when the extension shut omp down; the row's
  `planReview` says whether the review opened, was answered and how it closed. The review turns then run in print mode
  with `-c` in the same session folder; the trial's usage and transcript come from omp's session files.
- **Cache warming (0567, 0575):** every omp arm sets `providers.cacheWarming: streaming` (A0 is omp's defaults except
  this); no arm sets `idle`.
- **Harness versions (0574):** `a4.yaml` sets `DISABLE_AUTOUPDATER=1` and `a7.yaml` `OPENCODE_DISABLE_AUTOUPDATE=1` in
  the harness's environment, so Claude Code and opencode don't update themselves mid-run. The bench reads each arm's
  `--version` again after every trial; when it changed, that arm's remaining trials don't start, the other arms run on,
  and the run logs `stopped <arm>: <harness> --version now says <version>; the run started on <version> (0482 pins it);
  <n> <arm> trials not run` and exits 1.
- **A7 (0537):** `a7.yaml` runs `opencode run --model anthropic/claude-opus-5-5 --variant high --auto --format json
  <prompt>` (high reasoning as the other arms; `--auto` approves its permission asks, as A4 bypasses Claude Code's;
  check both flags against `opencode run --help` in the VM). Its only Anthropic sign-in is an API key: the bench reads
  it from `JOCASTA_A7_ANTHROPIC_API_KEY` and gives it to opencode's process alone as `ANTHROPIC_API_KEY`; every trial's
  environment drops both names otherwise. A7 runs only on the VM `JOCASTA-CLEAN`. Each attempt's spend is priced by the
  ledger at the pinned table; once A7's sum reaches $150 (or an A7 request's model is missing from the table, so its
  spend can't be counted) no further A7 trial starts, the other arms run on, and the run exits 4. A trial already
  running can pass the sum by its own cost; the key's own $150 limit at Anthropic is the backstop.
- **Thinking level:** pinned to `high` in every arm: `a0.yaml` passes `--thinking high` (omp's default, inherited by
  A1–A3), `a4.yaml` `--effort high`.
- **Prompt:** every arm gets `Fix the issue below in the repository in the current folder.` and a blank line, then
  the task's `problem_statement` verbatim (`TASK_PROMPT_PREFIX` in `evals/tasks.ts`).
- **Fixture:** the tree at `base_commit` without history, prepared once per task in `<cache>/<id>/fixture` from the
  shared bare repo `<cache>/repos/<owner>__<repo>.git` (`git fetch --depth 1 <repo_url> <base_commit>`), written through
  a throwaway index rather than `git archive` (which drops `export-ignore` files), without line-ending conversion;
  `prepared.json` (base commit and the patches' SHA-256) makes it idempotent. Each trial commits it fresh (`git init`).
- **Hidden tests:** `test_patch` applied in a scratch copy, each `hidden_test_files` entry copied to `<cache>/<id>/hidden/`
  (overlaid on the clean clone, replacing the agent's version); `gold.patch` and `test.patch` beside it. A deleted
  hidden file fails preparation (tasks.jsonl has none). The one hidden check is `test_command`; there are no visible
  checks, so `hiddenPassed` is the primary metric.
- **Grading (0574):** the trial's changes are committed on whatever HEAD the agent left (a branch it made or a
  detached HEAD included) and that commit is cloned clean for grading. Grading has one limit, 60 minutes for its setup
  and the hidden checks together; what runs past it is ended with its process tree and the row gets `hidden:
  timeout` 0. A grading setup that fails is run again on a clean clone of the fixture commit, within the same 60
  minutes (cut there, the row gets `hidden: timeout` 0 as a cut setup does): when it passes there, the trial's change
  broke it and the row counts as failed with `hidden: setup failed` 0; when it fails there too, the setup itself is
  broken and the trial is infra. The setup before the harness has its own 60-minute limit (0574, amended): a setup
  still running then is ended with its process tree and the trial is infra, retried (`conditions.json` records
  `setupLimitMinutes`; a resume with another value is refused).
- **Per-trial tools:** Python tasks run in a venv at the trial root (`<root>/venv`, made once, pip upgraded first),
  `venv/Scripts` first on PATH for setup and harness; grading makes a fresh venv for every Python trial,
  `<root>/gvenv` (`gvenv/Scripts` first on PATH), so nothing the agent installed in `venv` reaches the hidden tests
  (0574; 1–4 minutes more per Python trial). Node and Go setups run as they are (their caches are shared safely).
- **Order:** tasks × arms × reps shuffled with a seeded PRNG (seed 482), so arms interleave over the hours; `--pilot`
  is 5 tasks (seeded, 2 Python, 2 TypeScript/JavaScript, 1 Go) × 1 rep × the arms (35 trials with all seven arms).
- **Workers and pacing:** `--workers N` trials at once, each in its own worker thread; the quota pacer is awaited before
  each. It reads `omp usage --json` and waits only while every signed-in Anthropic account has its 5-hour or 7-day
  window at 85% or more (0543), until the first of them frees up; accounts are named `anthropic#1`, `anthropic#2` in
  the run's records. Never two trials of one task at once: a free worker takes the next trial in the seeded order whose
  task isn't running (hatch's and poetry's shared folders, npm/yarn/pnpm caches per project), so the order holds
  otherwise. A rerun with the same `--out` skips trials already in `results.jsonl`; a different plan (seed, arms, reps
  or tasks) in the same folder is refused, and so is a resume with another `--timeout`, profile or harness version
  (`<runDir> ran with --timeout 90, …; this resume has …; resume with the same flags or use a new --out`, 0574).
- **Trial roots:** each trial's root (overlay, `repo/`, `venv/`, `gvenv/`, the grading clone) is a short folder, `t`
  and 6 characters, in `--work` (default the OS temp folder); in the VM a short root such as `C:\t` keeps pnpm/yarn
  trees and Python under 260 characters. A layered arm's layer data is the exception: it goes in `%TEMP%\jl…` (see
  A6). Nothing in the root names the task (0508: agents list it); the trial's `setup.log` names the root.
- **Trial repos:** the fixture is the base commit's tree, committed whole (files its own ignore rules match too, as
  upstream tracks them) and tagged with upstream's nearest tag at the base commit (found once at preparation from a
  fetch of commits and tags only), so tools that read the version from git report upstream's, as in a full clone. The
  setup runs in that checkout and what it writes joins the fixture commit; hooks it installs stay off
  (`core.hooksPath` points at a folder that doesn't exist), so an arm that commits meets no hook another arm never does.
  The trial branch is `eval/trial-<rep>`: a task's id ends in its fix's PR number, and git output shows the branch
  (0508: the pilot's jest-16439 A0 cited the PR from it). `--timeout` is 90 minutes by default (0508, 0574).
- **Infra errors are not task failures:** a failed setup before the harness (or a grading setup that fails on the
  fixture commit too), a trial that ended on a rate limit or sign-in error, or a harness that ends before its first
  model response (not timed out) goes to `errors.jsonl` and is retried later, twice more at most. "Ended on" is read
  only from stderr and the harness's own end (EVL-2), never from the run's free text: omp's last assistant message
  with `stopReason: error` and its `errorMessage`; Claude Code's `result` with `is_error` and a `rate_limit_event`
  whose status isn't `allowed`; opencode's `error` events. A status code counts only with an error's context
  (`status 401`, `429 Too Many Requests`). A sign-in error counts when the harness ended on an error (non-zero, not
  timed out) or before any response; a rate limit or overload counts without a tool call, or after tool calls when
  the harness ended on an error while the pacer saw a full account (0543). Three sign-in failures in a row stop the
  run (exit 3) naming the harness: sign in again and rerun to resume.
- **Leak scan:** each transcript (the prompt removed) and the trial's saved sessions, subagents' records included, are
  searched for the PR's and issues' URLs, `github.com/<owner>/<repo>/pull/<n>`, `/commit/<merge_commit>` and its
  first 10 hex; matches go in `leak`. The run never stops for one; the analysis drops leaked trials in a sensitivity
  check.
- **Validation (`--validate`):** per task, no model: the fixture with its setup graded unchanged on a clean clone
  (must fail), then `gold.patch` applied and graded on a fresh clean clone 3 times (must pass each time). Tasks that
  fail are listed for a spare of the same language.
- **Dry run:** the trial count and limit, the first 15 trials of the order, each arm's harness command line, A6's
  layer switches and whether its layer can run here, A7's key, machine and limit, the pacer's rule, the ledger's
  files, and the cache, trial-root and results paths; the main run's dry run also names the follow-up study's size.
- **omp's own MCP servers (0587):** before any trial and in the dry run (not `--validate`), a run with an omp arm
  reads `mcp.json` and `.mcp.json` in the omp folder it uses (the `--profile`'s, else `PI_CODING_AGENT_DIR` or
  `~/.omp/agent`) as omp reads them, and refuses (exit 2) while a server there is on, naming each file and server.
  With a layered arm, a `jocasta` entry, even one turned off, or `jocasta` in `disabledServers` refuses too: omp keeps
  its folder's `jocasta` over the layer's own, so A6 would lose `report_result` and `check`. Servers omp imports from
  other tools' configs aren't checked.
- **Smoke run in the VM (EVL-15; about 12 paid trials):** a dry run with `--only` one Python, one TypeScript/JavaScript
  and one Go task, `--arms a4,a6,a7,f-keep-context --reps 1 --work C:\t --timeout 90`, then the same for real
  (`JOCASTA_EVALS_REAL=1`, A7's key set). Check: `errors.jsonl` is empty; A6's and `f-keep-context`'s rows have
  `planReview` requested, answered and shut down, and `layer.indexBuildMs`; A7's rows have `ledger.sessions` of 1 or
  more and a spend; the harness versions didn't change and no `stopped <arm>:` line was logged; no `t…` folder is
  left in `C:\t` and no `jl…` folder in `%TEMP%`.

**The follow-up study (0543, 0545, 0571, 0575), `--study followup`,** runs after the main run, in its own `--out`: 10
main tasks (seeded, Python, TypeScript/JavaScript and Go in the main set's proportions) × 2 reps × A6 and four arms,
100 trials, each arm paired with A6 by task and rep (A6 runs again in the same order, so the pairs share the hours and
the build): `f-index-off` (the code index off), `f-prewalk` (`omp --prewalk`, present in 18.18.6), `f-keep-context`
(the review answered "Approve and keep context") and `f-ponytail-if` (Ponytail with its "Running tests" rules inside
`<important if="you are running or writing tests">`, `evals/variants/ponytail-important-if.md`). pitchfork is no
comparison (0560), and neither are keep-alive pings (0575): a trial has no idle gap for a ping (the review is answered
inside the event and omp stops at the execution's end), so the task run reports them as not measured. The dry run
says `not compared: pitchfork (0560), keep-alive pings (no idle gap in a trial)`. The
`check` tool isn't a comparison either: the fixtures have no mise check, so A6's tool answers "No checks" and cached
checks are measured in H8 (0571). `--arms`,
`--reps`, `--set`, `--only` and `--pilot` are refused with it. **The keep rule** (0543, written before any run),
`bun evals/keep.ts --out <follow-up runDir>`: a tried feature stays when its paired cost per passing task (the ledger's
priced cost summed over the pairs ÷ hidden-test passes) is lower and its pass rate isn't significantly lower (McNemar's
exact test on the paired trials, Holm's correction over the four comparisons, α = 0.05); for `f-index-off` the feature
is A6's side.
Writes `keep.json`.

The single cases of `evals/cases/`:

`bun run evals` (`evals/`) runs the arms: `--variant a0` … `a4` (A0 omp alone, A1 + the pane config, A2 + plan
first, A3 + the review turns, A4 Claude Code), `--profile <name>` for an omp profile apart from his daily one (log it
in once), hidden graders (`hidden_checks` and a `hidden/` folder per case), and the same quota pacer (it waits while
every Anthropic account has its 5-hour or 7-day window at 85% or more). `bun run evals compare <A> <B>` adds the paired
statistics (task bootstrap, pass^3, McNemar, Wilcoxon on wall time, cost and tokens, Holm). A4's model id and
permission flag are Claude Code's documented ones; confirm them against `claude --help` in the VM. A5:
`bun evals/a5.ts record --out <dir>` records the service's state feed (`/ws`, through `bench/lib/service-feed.ts`)
during his sessions, and
`bun evals/a5.ts report` counts interventions, attention minutes and the batch time from it and the session files.
