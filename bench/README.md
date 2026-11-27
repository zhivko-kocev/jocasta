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
  `jocastaSha256`, the SHA-256 of the `jocasta` on PATH, so each row names the build it measured, 0533, 0576; since 0720
  every summary also carries the machine stamp `machine {os, kernel, cpu, memoryGb, gpu}`, `ompVersion` and
  `jocastaCommit`, the commit of the checkout the bench runs from, so the two machines' results tell themselves apart)
  and, where it has them, raw rows in `<out>/<name>.jsonl`. A script refuses an `--out` that already holds its `.jsonl`
  ("results are never overwritten"), so a rerun never mixes rows of two runs; tokscale parity rewrites its own rows.
- Medians, p95 and percentile-bootstrap 95% CIs (10,000 resamples, seed 482) come from `lib/stats.rs`.
- Cost is priced with one table for every arm and benchmark (0538): LiteLLM's `model_prices_and_context_window.json`,
  fetched once and committed in `prices/` with `prices/source.json` (its URL at commit `8fb9f2f0…759d` of
  2026-11-11, fetched 2026-11-18, and its SHA-256, which `lib/prices.rs` checks on every load). Cache read and cache
  write are priced apart, a 1-hour cache write at its own rate. The ledger (`lib/ledger.rs` on the product's
  `src/shared/ledger.rs`, which owns the rows, waste and peak) reads omp's session files,
  Claude Code's transcripts and opencode's `opencode export` JSON into rows per model request (time, model, uncached
  input, cache read, cache write, output, cost) and per tool call (tool, argument hash, file path, result size,
  error). In omp's session files a request is every assistant message and every side call omp records as
  `model_usage` (cache warming, compaction and the like). A session's copies (a forked or moved session's files, a
  subagent transcript copied along) count once. Per session it counts waste (re-reads of an unchanged file, the same
  call repeated, results of 50,000 bytes or more, repeated failures of one tool; cache misses: a cache write of 10,000
  tokens or more after more than 5 minutes without a request on the same provider and model; compactions: omp's own,
  `soft`, `shake`, `remote` and legacy, not Jocasta's `/handoff`, `method: "handoff"`, 0582) and the peak context
  (tokens and percent of the model's window from the table).

## Where they run

| Script | Model calls | Where |
|---|---|---|
| `h1-guard omp-verdicts`, `h1-guard evaluate` | none | anywhere with omp 19.2.0 installed (no command runs) |
| `h1-guard live-check` | 31 cheap-model turns (30 picks and a canary; a few cents) | his PC (its ACP client runs none of the picks, 0585, 0657) |
| `h1-guard git-guard-check` | none, but it runs the corpus's git lines (`--run`) | the VM `JOCASTA-CLEAN` only; not rerun since the VM, its VM results stand (0657); Windows only (0720) |
| `h7-longpath` | none | anywhere (git only, temp folders) |
| `h2-toast`, `h3-recovery`, `h5-startup` | none, except `h2 --start` | a sandbox Jocasta; `h2-toast` on the laptop only, since its notifications would show on the PC's live desktop (0720) |
| `h4-isolation` | the `h4 plant` thread's turn | his PC, in a sandbox Jocasta (0502) |
| `h6-memory`, `h8-parallel` | the threads' turns | his PC, in a sandbox Jocasta (0657) |
| `h9-footprint` | none | 0481's VM `JOCASTA-CLEAN` only (refused elsewhere); not rerun since the VM, its VM results stand (0657); Windows only (0720) |
| `footprint` | none (a stub agent stands in for omp) | his PC with no Jocasta running (`mise run footprint`; 0674); `--out <dir>` also writes `<dir>/footprint.json`; the budget is per OS, `footprint-budget.<os>.json` (0720) |
| `tokscale-parity` | none | his PC, with tokscale 4.21.0 from `D:/thesis/tools/tokscale` on the session's PATH (0657) |

The scripts that act on a running Jocasta (H2–H6, H8) refuse to start without a sandbox (0502, 0576): its own data
folder in `JOCASTA_DATA_DIR` (not the OS default) and its own config folder in `JOCASTA_CODING_AGENT_DIR`
(not `~/.jocasta/agent`: the config folder picks the
projects and omp's config, so a sandbox on his would act on his). Start the sandbox's `jocasta` with the same two. H2, H4 (its `h4 plant` thread), H6 and H8
start their threads as `jocasta thread new` does (a `cli` message on the bridge: no Allow, no boss key, 0544) and read them as
`jocasta thread list` does;
threads still open in plan mode and wait for the operator's approval of their plan in their pane. The bench never
closes threads or removes worktrees: those ask him first (0484), so H6 and H8 wait for Enter between runs.

Since 0652 the benchmarks run on his PC, on his own omp (19.2.0) and the Jocasta release built for the commit, and
the VM is removed; its results are in `D:/thesis/data/vm-results/`. Since 0657 every script has its place in the table
above; opencode 1.18.22 (A7, H5's and H6's competitor) and tokscale 4.21.0 are installed for the benchmarks only, in
`D:/thesis/tools/opencode` and `D:/thesis/tools/tokscale`, and a bench session puts their `node_modules/.bin` on its
PATH. The main run with A6 passes `--profile bench`: an omp profile whose `mcp.json` has no `jocasta` entry.

Before the full runs (0588), each script runs once in a sandbox Jocasta, each into its own throwaway `--out` (checks,
not results): every script's `--dry-run`, then `h3-recovery --runs 1` and `h5-startup --runs 1`; anything broken is
fixed before the long runs. The task benchmark refuses only an omp folder that would hide a layered arm's own
`jocasta` server (0587, 0652).

## Two machines (0720)

The thesis measures Jocasta on his PC (Linux, `linux-pc`) and on his Windows 11 ThinkPad E14 (`win-laptop`). Every
script writes into `<thesis>/data/<machine>/<date>/<script>`, one folder per script; each summary's `conditions` name
the machine (`machine`, `ompVersion`, `jocastaCommit`). Results stay outside the repo and uncommitted (the thesis kit's
`data/` is git-ignored).

| Script | PC (Linux) | Laptop (Windows 11) |
|---|---|---|
| `footprint`, `h3-recovery`, `h4-isolation`, `h5-startup`, `h6-memory`, `h7-longpath`, `h8-parallel` | yes | yes |
| `h2-toast` | no: its notifications would show on his live desktop | yes |
| `h9-footprint`, `h1-guard git-guard-check` | no: Windows, VM `JOCASTA-CLEAN` only | no: VM only, its VM results stand (0657) |
| the task benchmark | once `tasks.jsonl` is copied from his Windows PC | yes |

**The sandbox.** Every live script runs against a sandbox Jocasta: its own data, config, omp and mise folders, never
his. On the PC the sandbox also has no display and no session bus, and `notify-send`, `xdg-open` and `gdbus` are stubs
that only log to `$B1/stubs.log`, so nothing reaches his desktop. The sandbox's omp shares his omp login store (a link
to `agent.db`; its `settings` table is empty), not his omp settings. Each bench that starts threads gets a data folder
of its own, so it starts with none open. Linux, as run on 2026-11-27 (from the checkout, bash):

```bash
cargo build --release --bins
export B1=/tmp/jocasta-b1 D=~/thesis/data/linux-pc/2026-11-27
export JOCASTA_DATA_DIR=$B1/data JOCASTA_CODING_AGENT_DIR=$B1/config PI_CODING_AGENT_DIR=$B1/omp-agent MISE_CONFIG_DIR=$B1/mise
unset WAYLAND_DISPLAY DISPLAY DBUS_SESSION_BUS_ADDRESS
mkdir -p $B1/{data,config,omp-agent,mise,stubs,projects}
for t in notify-send xdg-open gdbus; do printf '#!/bin/sh\necho "$(date -Iseconds) %s $*" >> %s/stubs.log\n' $t $B1 > $B1/stubs/$t; chmod +x $B1/stubs/$t; done
export PATH=$B1/stubs:$PWD/target/release:$PATH
ln -s ~/.omp/agent/agent.db $PI_CODING_AGENT_DIR/agent.db
printf 'threads:\n  maxRunning: 8\n' > $B1/config/jocasta.yaml   # H6's 8 threads; the default 4 queues the fifth (0687)
jocasta setup --yes
for p in acme-api acme-web acme-h6; do git init -q -b main $B1/projects/$p; echo "# $p" > $B1/projects/$p/README.md; git -C $B1/projects/$p add .; git -C $B1/projects/$p commit -qm fixture; jocasta project add $B1/projects/$p; done
jocasta-bench h8-parallel --out $B1/h8-fixture --project acme-h8 --repo $B1/projects/acme-h8 --threads 2,4,8 --make-fixture
jocasta project add $B1/projects/acme-h8
# The sandbox Jocasta, when a script needs one running: a detached tmux session (no window).
tmux -L jb1 new-session -d -s jocasta "jocasta"   # attach with: tmux -L jb1 attach
```

The laptop (PowerShell, from the checkout after `mise trust; mise install --locked; mise exec -- bun install
--frozen-lockfile; mise run build`). Sign the sandbox's omp in once (`omp`, `/login`, `/exit`) with the variables set;
`jocasta setup --yes` also adds its Windows Terminal profile. A sandbox Jocasta runs in a second PowerShell window
with the same variables (`jocasta`).

```powershell
$B1 = "$env:TEMP\jocasta-b1"; $D = "D:\thesis\data\win-laptop\$(Get-Date -Format yyyy-MM-dd)"
$env:JOCASTA_DATA_DIR = "$B1\data"; $env:JOCASTA_CODING_AGENT_DIR = "$B1\config"
$env:PI_CODING_AGENT_DIR = "$B1\omp-agent"; $env:MISE_CONFIG_DIR = "$B1\mise"
$env:Path = "$PWD\target\release;$env:Path"
"data","config","omp-agent","mise","projects" | ForEach-Object { New-Item -ItemType Directory -Force "$B1\$_" | Out-Null }
Set-Content "$B1\config\jocasta.yaml" "threads:`n  maxRunning: 8"
jocasta setup --yes
omp
foreach ($p in "acme-api","acme-web","acme-h6") { git init -q -b main "$B1\projects\$p"; Set-Content "$B1\projects\$p\README.md" "# $p"; git -C "$B1\projects\$p" add .; git -C "$B1\projects\$p" commit -qm fixture; jocasta project add "$B1\projects\$p" }
jocasta-bench h8-parallel --out "$B1\h8-fixture" --project acme-h8 --repo "$B1\projects\acme-h8" --threads 2,4,8 --make-fixture
jocasta project add "$B1\projects\acme-h8"
```

**The scripts**, Linux as run, then the laptop's lines. Each live script starts its own sandbox Jocasta in a fresh
data folder (Linux: `export JOCASTA_DATA_DIR=$B1/data-<script>; tmux -L jb1 new-session -d -s jocasta "jocasta"`, and
`tmux -L jb1 kill-server` after; laptop: the same `$env:JOCASTA_DATA_DIR` in both windows, `jocasta` in the second).
Plans are approved in the thread's pane (attach, Alt+<n> for the thread, Enter on "Approve and execute").

```bash
jocasta-bench footprint --out $D/footprint
jocasta-bench h7-longpath --out $D/h7-longpath --root $B1/h7b
jocasta-bench h5-startup --out $D/h5-startup --tools jocasta,omp --cwd $B1/projects/acme-api
# data-h6b
jocasta-bench h6-memory --out $D/h6-memory --tools omp --threads 0,1,4,8 --project acme-h6 --runs 1 --cwd $B1/projects/acme-api
# data-h4: one small thread in each project gives A and B their project panes (ids in $JOCASTA_DATA_DIR/panes.json)
jocasta thread new acme-web "Reply with the word ready and change nothing." --title "h4 b"
jocasta thread new acme-api "Reply with the word ready and change nothing." --title "h4 a"
jocasta-bench h4-isolation --out $D/h4-isolation --project-a p-95000f820871e216 --pane-a p10 --pane-b p4
# data-h8; approve each thread's plan
jocasta-bench h8-parallel --out $D/h8-parallel --project acme-h8 --repo $B1/projects/acme-h8 --threads 2,4
```

```powershell
jocasta-bench footprint --out "$D\footprint"
# footprint-budget.windows.json predates the iced views (0721): measure it again here once, then commit the new file
Remove-Item bench\footprint-budget.windows.json; jocasta-bench footprint --write-budget
jocasta-bench h7-longpath --out "$D\h7-longpath" --root "$B1\h7"
jocasta-bench h5-startup --out "$D\h5-startup" --tools jocasta,omp --cwd "$B1\projects\acme-api"
jocasta-bench h2-toast --out "$D\h2-toast" --start acme-api
# data-h6
jocasta-bench h6-memory --out "$D\h6-memory" --tools omp --threads 0,1,4,8 --project acme-h6 --runs 1 --cwd "$B1\projects\acme-api"
# data-h4: the two threads as above; --project-a from `jocasta project list`, the panes from panes.json
jocasta-bench h4-isolation --out "$D\h4-isolation" --project-a <acme-api's id> --pane-a <acme-api's pane> --pane-b <acme-web's pane>
# data-h3: three small threads as for H4 first
jocasta-bench h3-recovery --out "$D\h3-recovery"
# data-h8
jocasta-bench h8-parallel --out "$D\h8-parallel" --project acme-h8 --repo "$B1\projects\acme-h8" --threads 2,4
```

H3 is not in the Linux list yet: on 2026-11-27 a killed thread pane was not started again and a restarted Jocasta
listed no threads, so it is re-run after that restore fix (same command, `--out $D/h3-recovery`, in `data-h3`).

**The task benchmark**, once `tasks.jsonl` is in `~/thesis/data/bench/tasks/` (copied from his Windows PC; A4 needs
Claude Code signed in, A7 the key in `JOCASTA_A7_ANTHROPIC_API_KEY`):

```bash
JOCASTA_EVALS_REAL=1 jocasta-bench evals bench --tasks ~/thesis/data/bench/tasks/tasks.jsonl --cache ~/thesis/data/bench/tasks/cache --out $D/tasks --reps 1
```

```powershell
$env:JOCASTA_EVALS_REAL = "1"; jocasta-bench evals bench --tasks D:\thesis\data\bench\tasks\tasks.jsonl --cache D:\thesis\data\bench\tasks\cache --out "$D\tasks" --reps 1
```

**Bringing the laptop's results home.** Copy `D:\thesis\data\win-laptop` to a stick (`Copy-Item -Recurse
D:\thesis\data\win-laptop E:\`) and on the PC `cp -r /run/media/$USER/<stick>/win-laptop ~/thesis/data/`; or, with
OpenSSH Server on the laptop, `scp -r <laptop>:D:/thesis/data/win-laptop ~/thesis/data/`. Nothing in `data/` is
committed.

## The scripts

- **H1 (0482, 0500, 0501, 0535, 0600, 0623).** Four scripts; the corpus is a separate file and this folder holds no command list.
  H1 runs on the build's omp, 19.2.0 (0535, 0623, 0663, 0690); `--any-version` is the exploration override.
  - `jocasta-bench h1-guard omp-verdicts --corpus <corpus.jsonl> --guard <guard.yml> --out <dir>
    [--git-guard <git-guard-check.jsonl>] [--omp <package dir>] [--any-version]` starts `bun
    bench/h1-guard/omp-verdicts.ts`, the oracle (0636: the one TypeScript file of the bench, because omp loads only on
    Bun), which imports omp's own `config/settings.ts`, `tools/bash.ts` and `tools/approval.ts` from the install (found
    from `omp` on PATH or Bun's
    global install; anything but 19.2.0 is refused), loads the guard as `--config` over omp's defaults, builds the
    real `BashTool` on a session stub that throws if execution is reached, and asks omp's `resolveApproval` for each
    line, as a pane's tool wrapper does. Nothing runs. omp's git guard refuses only when the embedded shell runs `git`:
    with `--git-guard` the refusals `h1-guard git-guard-check` measured decide for the lines it ran, and the
    reimplementation (`git_guard.rs`) for the rest; each line's `gitGuardSource` says which. A line omp allows and the
    git guard refuses is `deny` by `git-guard`. Writes `omp-verdicts.jsonl` (verdict, mechanism, the guard pattern,
    omp's approval alone, whether omp's critical list flagged it, the git guard's source, and the reimplementation's
    verdict) and prints where they disagree, by expected class and way of writing.
  - `jocasta-bench h1-guard git-guard-check --corpus <corpus.jsonl> --out <dir> (--dry-run | --run)` measures the git
    guard. It takes every line made only of git (any spelling the corpus uses, also inside `pwsh -c`/`cmd /c`), `cd`
    and `echo` (423 lines), and runs each twice through omp's own `executeBash` in `C:\work\demo`, with
    `gitGuard: true` and then `false` as the control, each on a fresh fixture: commits on `main`, the branches the
    corpus names with `feature/x` holding its own commit, a tag, a bare `origin` with everything pushed, a stash entry,
    `src/app.ts` modified and an untracked file, with git's config isolated in a temp folder. It refuses without
    `--run`, off the VM `JOCASTA-CLEAN`, or when `C:\work` holds more than the fixture. Writes
    `git-guard-check.jsonl` and `git-guard-check.json` (agreement with `git_guard.rs` per subcommand and way of writing,
    every disagreement listed).
  - `jocasta-bench h1-guard evaluate --corpus <corpus.jsonl> [--guard <guard.yml>] [--verdicts <omp-verdicts.jsonl>]
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
    the build's runs on omp 19.2.0 go in a folder of their own (`D:/thesis/data/bench/h1/omp-19.2.0/`). omp
    18.18.1 changed how its pattern rules combine (a deny matching any part of a compound line beats any prompt;
    critical patterns come before prompt rules), so `h1-guard omp-verdicts` is run again on the build's omp, 19.2.0 (on the
    0580 corpus 18.18.6's verdicts matched 18.18.2's line for line before 0589's move, and 18.20.2's matched 18.20.0's
    before 0623's, though 18.20.1 also matches a segment without its leading `NAME=value` words; 18.22.0 and 18.22.2 left the code
    that decides a verdict as 18.20.2 had it; 19.1.0 changed only how an allow rule treats leading `NAME=value` words, and the guard has no allow
    rule, and its oracle's session stub must now answer `toolCommands` with nothing; 19.1.1 left `tools/approval.ts` and the
    approval and `bash.patterns` code of `tools/bash.ts` as 19.1.0 had them; 19.2.0 changed one thing there: the
    flag pattern behind "an argument that runs text as code" (`sh -c`, `git -c alias…`) now matches one flag word
    and no longer runs across spaces (0690), so a quoted line with shell control and a later `-…c`/`-…e` word may no
    longer count as reinterpreted; checked from source, the corpus not yet rerun); and 18.14.0 rewrote
    the git guard (whose checkout git is in first; refusals for a failed save of uncommitted changes and for
    submodule recursion), so H1's measured refusals are the VM's on 18.13.1; with the VM gone `h1-guard git-guard-check --run` isn't run again (0657), and H1 reports them as such.
  - **The 0580 lines.** The guard's 0580 rules (combined force flags, trunk pushes, restore, update-ref and git's
    credentials, the graph's containers, omp's credentials and critical shapes, `gh repo edit --visibility`, the
    `*GIT*`/`*Git*` copies, merge and publish asks) each have labelled lines in the corpus's `dev` group, taken from
    `test/config/guard.test.ts` (the guard's author saw them, so they are `dev`, never held-out):
    `D:/thesis/data/bench/h1-dev-0580.jsonl` (206 lines; 201 after the builder's duplicates), built with
    `h1-guard build-corpus --dev` into `D:/thesis/data/bench/h1-corpus-0580.jsonl` (1,486 lines, SHA-256 `37d77a8f…0718516`).
    Its 1,285 held-out lines are the frozen corpus's, byte for byte (`276b223a…39b1de32`, `corpus.sha256`).
  - `jocasta-bench h1-guard live-check --corpus <corpus.jsonl> --out <dir> [--guard <guard.yml>] [--verdicts
    <omp-verdicts.jsonl>] [--model anthropic/claude-haiku-5-5] [--sample 30] [--seed 482]` is the design's check of the
    in-process verdicts against real omp: 30 random lines (seed 482), each one omp turn told to run that command once,
    read back as refused, asked or ran. The prompt shows the command verbatim between two marker lines (shown
    JSON-escaped, the model copied the escapes). A pick compares only when the call carries the command unchanged; the
    model may decline a destructive one (no-call), which leaves it out. **The recorder (0585)** is an ACP client
    (`acp_recorder.rs`): each turn is `omp --mode acp` with the script as its client, which advertises a terminal, so
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
    D:/thesis/data/bench/h1/omp-19.2.0/live`; check that the canary reached the recorder's terminal ("canary reached
    the recorder's terminal … nothing written"), that `live-check.json` names omp 19.2.0 and the recorder `ACP client
    terminal (0585)`, that the outcomes agree with the in-process verdicts, and that no `h1-live-canary.txt` is left.
    The 18.13.1 run (24/24) used omp's bash interceptor as the recorder, which 18.15.0 moved into the bash tool's
    approval.
- **H2, `jocasta-bench h2-toast --out <dir> [--runs 30] [--condition host|tick] [--start <project>]`.** Jocasta raises
  its notifications through the shell's own notification (0683), which hands them to Windows' notification service and
  answers `shown` (or `disabled`, `rate_limited`, `no_foreground_client`, `busy`). H2 looks at every pane's agent state
  every 200 ms through the service's `panes` command and records each change (t1, kept as a reference figure) and reads the service's
  notification log, `<data>/notifications.jsonl` (one line per call, written when the shell's reply comes back: `at`,
  `paneId`, `reason`, `title`, `body`, `result`). t0 is the pane's omp session file's `ask` call or plan proposal; an
  ask counts when omp completed its message (`completedAt`, else the entry's time), not when it started the request,
  so the model's generation is no longer inside t1−t0 and t3−t0 (the 0517/0520 rows include it; their session files
  weren't kept, so they can't be recomputed). **t3 is the moment the shell answered `shown` for the pane's `blocked`
  notification**: the notification reached Windows; how long Windows then takes to draw the banner isn't measured.
  Each row also names the shell's answer to the pane's first `blocked` notification (`notificationResult`), so a trial
  without t3 says why. **Conditions (0576):** `host` (the default): the service takes each thread's state as its omp
  pushes it over the bridge (0675); `tick`: the sandbox service started with `JOCASTA_HOSTS=off`, so it ignores those
  pushes and takes the state from its own refresh tick. H2 reads the switch from `/health` and refuses a
  mismatch. Without `--start`, trials come from threads started by hand (the boss's and the today pane left out).
  `--start` starts each trial as `jocasta thread new` does and counts only those threads' panes; the next starts once
  the last one's notification came, or after 5 minutes without one, which `h2-toast.json`'s `missed` lists. Writes
  `h2-toast.jsonl`, `h2-toast.json` (t1−t0, t3−t0 and the count of each notification result) and the run's lines of the log
  in `h2-notifications.jsonl`.
- **H3, `jocasta-bench h3-recovery --out <dir> [--runs 20] [--faults service,pane] [--restart jocasta]`.**
  Kills the service (Jocasta is one process since 0683: the service and the shell are the `jocasta` that
  `<data>/jocasta-service.json` names, and the panes run under it) or one omp pane and times recovery from the moment
  the kill returned (the lookups before it; the kill, the product's `killTree`, returns once the process tree is gone,
  about 50 ms after the bench's own kill did before 0573's fixes). Every fault counts as recovered when every
  workspace that had an omp pane before has an omp
  pane again, whatever its pane id (0522: the service relaunches panes under new ids); the service fault also needs a
  new service answering (0528: it relaunches the boss pane under a new id with its new key) and runs `--restart` in a
  pty of its own, which is the new window. Before 0576 the service
  fault was named `companion`; `h3-recovery.json`'s `"renamed": {"service": "companion"}` says so. The herdr fault of
  runs before 0683 (killing the surface's server) is gone with the server. The pane fault ends the process tree of the
  omp pane in the folder it killed longest ago, one at work first among equals, and counts it back when omp runs again
  in that workspace under another pane id; it waits, when it must, until 10.5 minutes have passed since that folder's
  last run ended (0528): 0521 restarts an omp that keeps ending in one folder after a wait that doubles from 10 s, so
  closer kills would time that wait, not recovery. After each service run the previous window ends, so one window is
  open at a time. A run after which Jocasta has closed
  itself (no service answers, and one still closing waited for up to a minute) counts not
  recovered with `stoppedItself` (0530): `--restart` starts Jocasta again in a new window, and the next run waits until a service answers and omp is back everywhere (`restartMs`; past 5 minutes
  H3 ends). Lost work: each pane folder's git status and diff hash, and each session file still parses: every
  non-empty line is JSON and a `session` entry is in it, wherever it is (0523; omp 18.13.1 wrote a `title` entry
  first). The plugin's `today` pane (0553) is left out of every pane check: it is no thread, and the service opens it
  again under a new id. Logging off and on is done by hand with the same checks. Writes `h3-recovery.jsonl` and
  `h3-recovery.json`.
- **H4, `jocasta-bench h4-isolation --out <dir> --project-a <p-…> --pane-a <pane> --pane-b <pane> [--canaries 50]
  [--runs 100] [--plant-timeout 30] [--skip-plant]`.** Runs on his PC in a sandbox Jocasta (0502). Plants the
  canaries through the service (0576): it starts a thread `h4 plant` in A as `jocasta thread new`
  does, whose brief tells it to call `report_result` once with exactly the canaries as decisions (`<topic> codename` =
  the token, why `H4 canary`); the service adds them to A's history index (0539, 0603). The operator approves the
  thread's plan in its pane; once `jocasta thread list` shows its report (within `--plant-timeout` minutes), H4 has
  nothing to wait for. Then it queries `history_search` from B's pane over the service's bridge (0675; as that pane:
  its key from the sandbox's `<data>/panes.json`; a pane without one is refused: `pane <id> has no key in
  <data>/panes.json: pass a pane Jocasta started`) and checks every answered reply for every canary; errored replies
  are counted apart and aren't checks. The same queries from A's pane are the required positive control: the
  rule-of-three bound is given only when it found canaries, else the run is `inconclusive`. Scans every other
  project's `mnemopi.db` and `mnemopi.db-wal` for the canaries. Prints e.g. `0 leaks in 4,900 answered checks (2
  queries errored); positive control found canaries in 92/100 queries; 95% upper bound 3/4,900`. Writes
  `h4-canaries.json` and `h4-isolation.json`.
- **H5, `jocasta-bench h5-startup --out <dir> [--runs 20] [--tools jocasta,omp,claude,opencode] [--cwd <repo>]
  [--ready tool=<regex>]`.** Cold (after 30 s with the tool's processes gone; an OS-cold start needs a reboot per
  run) and warm starts. Jocasta ends at the same point as omp alone (0576): `readyMs` is the boss pane's screen
  matching omp's ready pattern; each row also has `bossIdleMs` (the shell reporting the boss's omp idle) and `serviceMs`
  (the new service answering `/health`; rows before 0576 call it `companionMs`, and the summary reads either).
  Between runs H5 closes Jocasta as he does (by closing its window, 0683) and waits until
  `jocasta-service.json` is gone and the `jocasta` it started has exited; after 60 s it ends what is left. The others start
  in a pty of the bench's own (Bun's PTY is POSIX-only; never his session
  nor the sandbox's) and are timed to their ready pattern. Each starts as a command line runs its name
  (`toolCommand`: `Bun.which`, with npm's `.cmd` shim for an `.exe` replaced by that `.exe`), because a bare
  npm extensionless shim is one Windows cannot start. The default patterns (`lib/launch.rs`) were
  each seen in the VM on 2026-10-28: omp's prompt after 3.6 s, unsigned opencode's after 9.8 s cold while the VM was
  busy, and Claude Code's after 3.7 s (its footer reads "shift+tab to cycle" in its default auto mode, "? for
  shortcuts" in the others). Claude Code asks to trust each new folder, which a bench pane can't answer, so with it in
  the tools H5 and H6 need `--cwd` at a folder trusted once (in the VM it was `C:\jc\bench-cwd`); a real run without one
  stops before any work. Writes `h5-startup.jsonl` and `h5-startup.json`.
- **H6, `jocasta-bench h6-memory --out <dir> [--threads 0,1,4,8] [--project <name>] [--tools omp,claude,opencode]
  [--idle 120] [--turn]`.** Working set and private bytes of Jocasta's processes with 0–8 threads:
  the one Jocasta process tree (service, shell and the panes with their omp and MCP processes), and each thread pane alone; each
  competitor in a bench pane after idle, and with `--turn` after one prompt. opencode runs unsigned (0506: its
  Anthropic sign-in takes only an API key), so it gets no prompt and `skipped` says why. Writes `h6-memory.jsonl` and
  `h6-memory.json`.
- **H7, `jocasta-bench h7-longpath --out <dir> [--lengths 100,…,240] [--root <dir>] [--home <dir>]
  [--location name=<template>]`.** Fixture repos whose deepest file has a relative path of L characters, checked out
  with `git worktree add` at Jocasta's `<parent>/<name>.wt/<slug>` and herdr's `<home>/.herdr/worktrees/<name>/<slug>` (a competitor's, kept as a label)
  and Claude Code's `<repo>/.claude/worktrees/<slug>` (confirm on the machine that runs it), with Git's default settings. vibe-kanban's
  folder is added with `--location` once confirmed. Writes `h7-longpath.jsonl` and `h7-longpath.json` (the largest
  passing L per location).
- **H8, `jocasta-bench h8-parallel --out <dir> --project <name> --repo <checkout> [--threads 2,4,8] [--layer on|off]
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
  priced by the ledger), the peak working set and private bytes of Jocasta's processes (its one
  process tree; sampled every 10 s) and, layer on, the run's shared
  check hits from the service's log (`readLayerCounters`). Writes `h8-parallel.jsonl` and
  `h8-parallel.json`.
- **tokscale parity (0538), `jocasta-bench tokscale-parity --out <task-benchmark runDir>`.** The ledger's parser check,
  bench-only: per saved session of the run, the ledger's uncached input, cache read, cache write and output against
  `tokscale models --json --group-by client,session,model` (omp's saved folders passed as `TOKSCALE_EXTRA_DIRS`, in a
  throwaway `TOKSCALE_CONFIG_DIR`; Claude Code's and opencode's records where they live on the machine). Tokens must
  match exactly; dollars aren't compared. tokscale is pinned at v4.21.0 (2026-11-02, MIT), the Windows binary of npm's
  `tokscale@4.21.0` (its `@tokscale/cli-win32-x64-msvc` package; the GitHub release has no files), installed on his PC
  for the benchmark only in `D:/thesis/tools/tokscale` (0657): without it on PATH the script stops and
  `tokscale_parity_tests.rs`'s live case skips, saying so. Writes
  `tokscale-parity.jsonl` (rewritten on each check) and `tokscale-parity.json`; exits 1 on a mismatch and 2 when it
  compared nothing (`bench: no saved sessions to compare in <runDir>\sessions (results.jsonl names <n> trials)`).
- **H9, `jocasta-bench h9-footprint --out <dir> --install "<command>" --uninstall "<command>" [--between "<command>"]
  [--roots <dir,…>]`.** Runs only on the VM `JOCASTA-CLEAN` and refuses anywhere else before anything runs.
  Snapshots files (the default roots include `%USERPROFILE%\.config`, where setup writes its mise files, 0468) and the
  registry before install, after install, after `--between` (0481's setup, first run and a thread, when given) and
  after uninstall; reports what install added, what setup and use added, and what uninstall left (all, and what names
  jocasta, herdr (a competitor's name), omp or mise as a word). Prerequisites and minutes to the first task go in the run's notes by hand.
  Writes `h9-footprint.json`, `h9-install-added.jsonl`, `h9-use-added.jsonl` and `h9-uninstall-left.jsonl`;
  `h9-footprint.json`'s conditions carry the installed `jocasta.exe`'s `--version` line and `jocastaSha256`, read after
  the first install (from the install folder when this process's PATH predates the install's PATH entry, 0576).

## The task benchmark

<!-- EVL -->

**`jocasta-bench evals bench --tasks <tasks.jsonl> --cache <dir> --out <runDir> [--study main|followup]
[--arms a0,a1,a2,a3,a4,a6,a7] [--reps 3] [--set main] [--only id,id] [--pilot] [--workers 2] [--seed 482]
[--timeout 90] [--profile <omp profile>] [--work <dir>] [--validate] [--dry-run]`** runs the 40 curated tasks of
`D:/thesis/data/bench/tasks/tasks.jsonl` through the eval runner's `runTrial` (same transcripts, usage and cost, line
counts, findings and clean-clone hidden grading as `jocasta-bench evals`). The main run (0543) is 30 tasks × 3 reps × 7 arms
= 630 trials: A0 omp alone, A1 + the pane config, A2 + plan first, A3 + the review, A4 Claude Code, **A6** = A3 + the
harness-OS layer, **A7** = opencode bare. Results go to `D:/thesis/data/bench/tasks/runs/<run>/`: `results.jsonl`
(the runner's row plus `task`, `arm`, `language`, `attempt`, `trialIndex`, `leak` and `ledger`, the trial's ledger
totals: tokens, priced cost, cache read share, waste counts, peak context; a row in whose output or exports A7's key
showed has `keyExposed: true`, the key replaced
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

- **A6 (0543, 0567):** `a6.yaml` extends A3 with `layer: true` (the `check` thread tool is always on, 0571), and plans
  first as Jocasta's threads do: not A3's `--plan-yolo`, but omp's interactive plan review answered "Approve and
  execute" (`plan_review: execute`); `modelRoles.default` and `.plan` are pinned to `anthropic/claude-opus-5-5:high`, so
  the review's planning role and execution tier stay on the arms' model. The layer is Jocasta's build, not the
  bench's: each A6 trial runs `src/frontier/layer/bench.rs`, linked into `jocasta-bench` (`LAYER_MODULE` in `evals/layer.rs`), whose
  `BENCH_LAYER.set_up(trial)` returns the omp flags, overlay settings and environment that bring in the task file's brief,
  `report_result`, the cached checks and `history_search` (0604; a trial has no past sessions, so it answers "Nothing
  in <project>'s history for that."), and a `close()`. The layer's data (its checks and endpoint file) goes in a
  folder of its own per trial, `%TEMP%\jl…`, removed at the trial's end; the trial roots stay in `--work` (EVL-1). A
  checkout without the layer build is refused before any trial ("A6 needs Jocasta's layer build", exit 2); the dry run
  marks such an arm `NOT RUNNABLE`. A6 has no code index (0605): relationship questions go to omp's `lsp`, and nothing
  is built before the trial's clock.
- **The plan review (0567, `evals/plan_review.rs`):** omp 18.20.2 offers "Approve and execute" (the session is cleared
  before the plan-approved prompt) and "Approve and keep context" only in its interactive plan review; print mode has
  only `--plan-yolo` and ACP's approval, both keeping the planning context. The interactive review emits
  `plan_review_requested` to extensions, and `ctx.resolvePlanReview(reviewId, "execute" | "keep")` answers it as if he
  had picked it. So a `plan_review` arm's first turn runs `omp <flags> --session-dir <root>/sessions --config <overlay>
  -e <root>/plan-review.js <prompt>` interactive (it needs a TTY) in a pty of the bench's own
  its overlay adding `plan.defaultOnStartup: true`; the per-trial extension answers the
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
  both flags checked against `opencode run --help` 1.18.22 on his PC on 2026-11-23). Its only Anthropic sign-in is an API key: the bench reads
  it from `JOCASTA_A7_ANTHROPIC_API_KEY` and gives it to opencode's process alone as `ANTHROPIC_API_KEY`; every trial's
  environment drops both names otherwise. A7 ran only on the VM `JOCASTA-CLEAN` until 0657; it runs on his PC now. Each attempt's spend is priced by the
  ledger at the pinned table; once A7's sum reaches $150 (or an A7 request's model is missing from the table, so its
  spend can't be counted) no further A7 trial starts, the other arms run on, and the run exits 4. A trial already
  running can pass the sum by its own cost; the key's own $150 limit at Anthropic is the backstop.
- **Thinking level:** pinned to `high` in every arm: `a0.yaml` passes `--thinking high` (omp's default, inherited by
  A1–A3), `a4.yaml` `--effort high`.
- **Prompt:** every arm gets `Fix the issue below in the repository in the current folder.` and a blank line, then
  the task's `problem_statement` verbatim (`TASK_PROMPT_PREFIX` in `evals/tasks.rs`).
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
- **Pausing (0652):** while a file named `PAUSE` is in the run's `--out` folder, no trial starts; trials already
  running finish. Deleting the file goes on. The settings trials (`evals settings`) do the same with their `--dir`.
  A trial also starts only with 4 GB of memory available, as the settings trials' do.
- **Trial roots:** each trial's root (overlay, `repo/`, `venv/`, `gvenv/`, the grading clone) is a short folder, `t`
  and 6 characters, in `--work` (default the OS temp folder); on Windows a short root such as `C:\t` keeps pnpm/yarn
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
- **Dry run:** the trial count and limit, the first 15 trials of the order, each arm's harness command line, whether
  A6's layer can run here, A7's key and limit, the pacer's rule, the ledger's
  files, and the cache, trial-root and results paths; the main run's dry run also names the follow-up study's size.
- **omp's own MCP servers (0587, 0652):** before any trial and in the dry run (not `--validate`), a run with a
  layered arm reads `mcp.json` and `.mcp.json` in the omp folder it uses (the `--profile`'s, else
  `PI_CODING_AGENT_DIR` or `~/.omp/agent`) as omp reads them, and refuses (exit 2) while a `jocasta` entry, even one
  turned off, or `jocasta` in `disabledServers` is there: omp keeps its folder's `jocasta` over the layer's own, so A6
  would lose `report_result` and `check`. Since 0652 the benchmarks run on his own omp folder, and the servers it
  turns on load in every omp arm alike, as his skills do.
- **Smoke run before the main run (EVL-15, 0588; about 12 paid trials; in the VM until 0652):** a dry run with `--only` one Python, one TypeScript/JavaScript
  and one Go task, `--arms a4,a6,a7,f-keep-context --reps 1 --work C:\t --timeout 90`, then the same for real
  (`JOCASTA_EVALS_REAL=1`, A7's key set). Check: `errors.jsonl` is empty; A6's and `f-keep-context`'s rows have
  `planReview` requested, answered and shut down; A7's rows have `ledger.sessions` of 1 or
  more and a spend; the harness versions didn't change and no `stopped <arm>:` line was logged; no `t…` folder is
  left in `C:\t` and no `jl…` folder in `%TEMP%`.

**The follow-up study (0543, 0545, 0571, 0575, 0605, 0608, 0620), `--study followup`,** runs after the main run, in its own `--out`: 10
main tasks (seeded, Python, TypeScript/JavaScript and Go in the main set's proportions) × 2 reps × A6, A1 and six arms,
160 trials, each arm paired by task and rep with its own baseline (A6 and A1 run again in the same order, so the pairs
share the hours and the build): `f-keep-context` (the review answered "Approve and keep context"), `f-keep-smol` (the
same, the work that follows done on the `smol` role: `plan_review_role: smol`, passed as `resolvePlanReview`'s
`{ role }`), `f-ponytail-if` and `f-advisor` (omp's advisor on, its role at Fable 5.2 medium, every third update; A0
pins it off for every other arm, 0619) against A6, and `f-prewalk-clean` (A1 with `omp --prewalk`, present in 18.20.2;
not A6, which starts in plan mode, where omp turns prewalk's automatic switch off) and `f-think-auto` (A1 with
`--thinking auto`, the last `--thinking` winning over A0's `high`) against A1. Before `f-advisor` runs, one pilot trial
(0588's single run) must show an advisor note landing inside A6's turn; if none can, the arm stops and he is asked
(0620, 0020). A `--prewalk` trial's results
row has `prewalkSwitched`: whether omp's session file has a `model_change` entry with the role `prewalk`, which omp
writes only when the switch fires (`session/prewalk.ts` `advanceAtTurnEnd` → `setModelTemporary` with
`{ prewalk: true }`, `session/model-controls.ts`). `jocasta-bench evals keep` pairs each comparison with its baseline (`baseline`
in `evals/followup.rs`). `f-ponytail-if` is Ponytail with its "Running tests" rules inside
`<important if="you are running or writing tests">` (`evals/variants/ponytail-important-if.md`). pitchfork is no
comparison (0560), and neither are keep-alive pings (0575): a trial has no idle gap for a ping (the review is answered
inside the event and omp stops at the execution's end), so the task run reports them as not measured. The dry run
says `not compared: pitchfork (0560), keep-alive pings (no idle gap in a trial)`. The
`check` tool isn't a comparison either: the fixtures have no mise check, so A6's tool answers "No checks" and cached
checks are measured in H8 (0571). `--arms`,
`--reps`, `--set`, `--only` and `--pilot` are refused with it. **The keep rule** (0543, written before any run),
`jocasta-bench evals keep --out <follow-up runDir>`: a tried feature stays when its paired cost per passing task (the ledger's
priced cost summed over the pairs ÷ hidden-test passes) is lower and its pass rate isn't significantly lower (McNemar's
exact test on the paired trials, Holm's correction over the three comparisons, α = 0.05), the feature being each arm's
side.
Writes `keep.json`.

The single cases of `evals/cases/`:

`jocasta-bench evals` (`evals/`) runs the arms: `--variant a0` … `a4` (A0 omp alone, A1 + the pane config, A2 + plan
first, A3 + the review turns, A4 Claude Code), `--profile <name>` for an omp profile apart from his daily one (log it
in once), hidden graders (`hidden_checks` and a `hidden/` folder per case), and the same quota pacer (it waits while
every Anthropic account has its 5-hour or 7-day window at 85% or more). `jocasta-bench evals compare <A> <B>` adds the paired
statistics (task bootstrap, pass^3, McNemar, Wilcoxon on wall time, cost and tokens, Holm). A4's model id and
permission flag are Claude Code's documented ones, checked against `claude --help` on his PC on 2026-11-23. A5:
`jocasta-bench evals a5 record --out <dir>` records the service's state feed (`/ws`, through `bench/lib/service_feed.rs`)
during his sessions, and
`jocasta-bench evals a5 report` counts interventions, attention minutes and the batch time from it and the session files.
