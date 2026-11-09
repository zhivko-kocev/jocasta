# Benchmarks (0482)

The harness of record [0482](../docs/decisions/0482-benchmarks-pre-registered.md), the thesis's pre-registration: what
runs, how often and what is measured is fixed there, as amended for the harness-OS rerun by 0533, 0537, 0538, 0543,
0545, 0546 and 0552. A change to the procedure before the run is a new record.

- Fixtures and raw results live outside the repo, in `D:/thesis/data/`. Every script takes `--out <dir>`; a folder
  inside the repo is refused.
- Every script takes `--runs <n>` (its default is the pre-registered count) and `--dry-run`, which prints what it would
  start, kill and measure, and runs nothing.
- Each writes `<out>/<name>.json` (the summary, with the run's conditions: time and local hour, OS, CPU, memory and the
  tools' versions, no host or user names) and, where it has them, raw rows in `<out>/<name>.jsonl`.
- Medians, p95 and percentile-bootstrap 95% CIs (10,000 resamples, seed 482) come from `lib/stats.ts`.
- Cost is priced with one table for every arm and benchmark (0538): LiteLLM's `model_prices_and_context_window.json`,
  fetched once and committed in `prices/` with `prices/source.json` (its URL at commit `e58354f0…398f` of
  2026-11-08, fetched 2026-11-09, and its SHA-256, which `lib/prices.ts` checks on every load). Cache read and cache
  write are priced apart, a 1-hour cache write at its own rate. The ledger (`lib/ledger.ts` on the product's
  `src/shared/ledger.ts`, which owns the rows, waste and peak) reads omp's session files,
  Claude Code's transcripts and opencode's `opencode export` JSON into rows per model request (time, model, uncached
  input, cache read, cache write, output, cost) and per tool call (tool, argument hash, file path, result size,
  error), with per-session waste counts (re-reads of an unchanged file, the same call repeated, results of 50,000
  bytes or more, repeated failures of one tool, cache misses: a cache write of 10,000 tokens or more after more than
  5 minutes idle, compactions) and the peak context (tokens and percent of the model's window from the table).

## Where they run

| Script | Model calls | Where |
|---|---|---|
| `h1-guard/omp-verdicts.ts`, `h1-guard/evaluate.ts` | none | anywhere with omp 18.13.1 installed (no command runs) |
| `h1-guard/live-check.ts` | 31 cheap-model turns (30 picks and a canary; a few cents) | 0481's VM |
| `h1-guard/git-guard-check.ts` | none, but it runs the corpus's git lines (`--run`) | the VM `JOCASTA-CLEAN` only |
| `h7-longpath.ts` | none | anywhere (git only, temp folders) |
| `h2-toast.ts`, `h3-recovery.ts`, `h5-startup.ts` | none, except `h2 --start` | a sandbox Jocasta |
| `h4-isolation.ts` | Graphiti's extraction of the planted facts | 0481's VM |
| `h6-memory.ts`, `h8-parallel.ts` | the threads' turns | 0481's VM |
| `h9-footprint.ts` | none | 0481's VM (Windows) |
| `tokscale-parity.ts` | none | the VM, with tokscale 4.21.0 on PATH |

The scripts that act on a running Jocasta (H2–H6, H8) refuse to start without a sandbox: `JOCASTA_DATA_DIR` set and
`JOCASTA_HERDR_SESSION` naming a session that isn't `jocasta` (or `jarvis`). The boss's tools (`thread_start`,
`thread_status`) need the boss key from the boss pane's environment in `JOCASTA_BOSS_KEY` (0484). The bench never
closes threads or removes worktrees: those ask him first (0484), so H6 and H8 wait for Enter between runs.

## The scripts

- **H1 (0482, 0500, 0501).** Four scripts; the corpus is a separate file and this folder holds no command list.
  - `bun bench/h1-guard/omp-verdicts.ts --corpus <corpus.jsonl> --guard <guard.yml> --out <dir>
    [--git-guard <git-guard-check.jsonl>] [--omp <package dir>] [--any-version]` imports omp's own
    `config/settings.ts`, `tools/bash.ts` and `tools/approval.ts` from the install (found from `omp` on PATH or Bun's
    global install; anything but 18.13.1 is refused), loads the guard as `--config` over omp's defaults, builds the
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
    `D:/thesis/data/bench/h1/{pre-0485,0485,campaign}/`, omp 18.13.1's in `D:/thesis/data/bench/h1/omp-18.13.1/`.
  - `bun bench/h1-guard/live-check.ts --corpus <corpus.jsonl> --out <dir> [--guard <guard.yml>] [--verdicts
    <omp-verdicts.jsonl>] [--model anthropic/claude-haiku-5-5] [--sample 30] [--seed 482]` is the design's check of the
    in-process verdicts against real omp: 30 random lines (seed 482), each one `omp -p --mode json --tools bash` turn
    told to run that command once, read back as refused, asked (print mode fails a prompt verdict) or ran. The prompt
    shows the command verbatim between two marker lines (shown JSON-escaped, the model copied the escapes). A pick
    compares only when the call carries the command unchanged; the model may decline a destructive one (no-call), which
    leaves it out. omp can't swap the shell its bash tool runs, so a second `--config` turns on omp's bash interceptor
    with a rule that blocks every call after the approval gate; launch and async are off. Before any model call it
    checks with omp's code that the settings layer so and the interceptor blocks every pick, then runs a canary that
    must come back blocked with no file written. `--dry-run` prints the picks, the model, the command line and the
    overlay. Writes `live-check.jsonl` and `live-check.json`.
- **H2, `bun bench/h2-toast.ts --out <dir> [--runs 30] [--condition events|tick] [--start <project>]`.** There is
  no companion app (0552): Jocasta raises its notifications through herdr's `notification.show`, which herdr hands to
  Windows' notification service through the foreground client and answers `shown` (or `disabled`, `rate_limited`,
  `no_foreground_client`, `busy`). H2 records herdr's `pane.agent_status_changed` pushes (t1) and reads the service's
  notification log, `<data>/notifications.jsonl` (one line per call, written when herdr's reply comes back: `at`,
  `paneId`, `reason`, `title`, `body`, `result`); t0 is the `ask` call or plan proposal in the pane's omp session file.
  **t3 is now the moment herdr answered `shown` for the pane's `blocked` notification**: the notification reached
  Windows. The companion's toast broadcast (the old t2) and Jocasta's toast window (0520's t3) are gone; how long
  Windows then takes to draw the banner isn't measured. Each row also names herdr's answer to the pane's first
  `blocked` notification (`notificationResult`), so a trial without t3 says why. `--start` starts each trial with
  `thread_start`. `--condition tick` only labels the run: cutting herdr's event stream for the service is arranged by
  hand. Writes `h2-toast.jsonl`, `h2-toast.json` (t1−t0, t3−t0 and the count of each herdr answer) and the run's lines
  of the log in `h2-notifications.jsonl`.
- **H3, `bun bench/h3-recovery.ts --out <dir> [--runs 20] [--faults herdr,companion,pane] [--restart jocasta]`.**
  Kills herdr's server, the companion, or one omp pane and times recovery. Every fault counts as recovered when every
  workspace that had an omp pane before has an omp pane again, whatever its pane id (0522: the service relaunches
  panes under new ids); the companion fault also needs a new companion answering (0528: the new companion relaunches
  the boss pane under a new id with its new key). For the herdr fault each row also gives `oldPaneIdsMs`, the time
  until herdr first listed exactly the old pane ids (null when no 250 ms poll saw them), summarized beside
  `recoveryMs`. The pane fault kills the omp pane in the folder it killed longest ago, one at work first among equals,
  and waits, when it must, until 10.5 minutes have passed since that folder's last run ended (0528): 0521 restarts an
  omp that keeps ending in one folder after a wait that doubles from 10 s, so closer kills would time that wait, not
  recovery. After each companion run the restart's client ends, root first and never the new companion's tree, so one
  hidden client is attached at a time. A run after which Jocasta has closed itself (no companion answers and herdr
  lists no panes) counts not recovered with `stoppedItself` (0530): `--restart` starts Jocasta again, its client left
  attached in place of the terminal's, and the next run waits until a companion answers and omp is back everywhere
  (`restartMs`; past 5 minutes H3 ends). Lost work: each pane folder's git status and diff hash, and each session file
  still parses: every non-empty line is JSON and a `session` entry is in it, wherever it is (0523; omp 18.13.1 writes
  a `title` entry first). The plugin's `today` pane (0553) is left out of every pane check: it is no thread, and the
  service opens it again under a new id. Logging off and on is done by hand with the same checks. Writes
  `h3-recovery.jsonl` and `h3-recovery.json`.
- **H4, `bun bench/h4-isolation.ts --out <dir> --project-a <p-…> --pane-b <pane> [--pane-a <pane>]
  [--canaries 50] [--runs 100] [--skip-plant]`.** Plants canary facts as Graphiti episodes in A's graph, then queries
  `graph_search` from B's pane through the service's MCP endpoint and checks every reply for every canary; `--pane-a`
  is the positive control. Scans every other project's `mnemopi.db` for the canaries (planting into A's bank is
  telling A's pane to remember `h4-canaries.json`). Writes `h4-canaries.json` and `h4-isolation.json` (leaks, checks,
  rule-of-three bound).
- **H5, `bun bench/h5-startup.ts --out <dir> [--runs 20] [--tools jocasta,omp,claude,opencode] [--cwd <repo>]
  [--ready tool=<regex>]`.** Cold (after 30 s with the tool's processes gone; an OS-cold start needs a reboot per
  run) and warm starts. Jocasta: companion `/health` and the boss pane idle. The others start in a tab of a separate
  bench herdr session (`--pty-session bench-pty`; Bun's PTY is POSIX-only) and are timed to their ready pattern. Each
  starts as a command line runs its name (`toolCommand`: `Bun.which`, with npm's `.cmd` shim for an `.exe` replaced
  by that `.exe`), because herdr 0.9.5 picks npm's extensionless shim for Git Bash, which Windows can't start. The
  default patterns (`lib/launch.ts`) were each seen in the VM on 2026-10-28: omp's prompt after 3.6 s, unsigned
  opencode's after 9.8 s cold while the VM was busy, and Claude Code's after 3.7 s (its footer reads "shift+tab to
  cycle" in its default auto mode, "? for shortcuts" in the others). Claude Code asks to trust each new folder, which
  a bench pane can't answer, so with it in the tools H5 and H6 need `--cwd` at a folder trusted once (in the VM,
  `C:\jc\bench-cwd`); a real run without one stops before any work. Writes `h5-startup.jsonl` and `h5-startup.json`.
- **H6, `bun bench/h6-memory.ts --out <dir> [--threads 0,1,4,8] [--project <name>] [--tools omp,claude,opencode]
  [--idle 120] [--turn]`.** Working set and private bytes of the companion's and herdr's trees with 0–8 threads, and
  each thread pane alone; each competitor in a bench pane after idle, and with `--turn` after one prompt. opencode runs
  unsigned (0506: its Anthropic sign-in takes only an API key), so it gets no prompt and `skipped` says why. Writes
  `h6-memory.jsonl` and `h6-memory.json`.
- **H7, `bun bench/h7-longpath.ts --out <dir> [--lengths 100,…,240] [--root <dir>] [--home <dir>]
  [--location name=<template>]`.** Fixture repos whose deepest file has a relative path of L characters, checked out
  with `git worktree add` at Jocasta's `<parent>/<name>.wt/<slug>`, herdr's `<home>/.herdr/worktrees/<name>/<slug>`
  and Claude Code's `<repo>/.claude/worktrees/<slug>` (confirm in the VM), with Git's default settings. vibe-kanban's
  folder is added with `--location` once confirmed. Writes `h7-longpath.jsonl` and `h7-longpath.json` (the largest
  passing L per location).
- **H8, `bun bench/h8-parallel.ts --out <dir> --project <name> --repo <checkout> [--threads 2,4,8] [--layer on|off]
  [--make-fixture]`.** Pairs of threads edit one file or one interface from both sides; reports wrong-thread edits,
  overlaps flagged before merge (`thread_status`), `git merge-tree` conflicts and conflicts found only at merge. A
  thread counts finished when `thread_status` shows it idle or done and its branch has a commit beyond `main` (0529),
  so one that asks instead runs to the 60-minute limit. **K11 (0543):** `--layer on` and `--layer off` run 4 threads
  at once, 3 runs each (the defaults with `--layer`), with the harness-OS layer switched in the sandbox by hand (the
  flag labels the run); each run adds the cost per finished thread (each thread pane's omp session file as herdr
  reports it, its subagents' files beside it, priced by the ledger), the peak working set and private bytes of the
  service's and herdr's process trees (threads included, sampled every 10 s) and, layer on, the shared check hits and
  index reuse, which the layer build counts (`readLayerCounters`; until it exists a layer-on run stops before any
  thread starts). Writes `h8-parallel.jsonl` and `h8-parallel.json`.
- **tokscale parity (0538), `bun bench/tokscale-parity.ts --out <task-benchmark runDir>`.** The ledger's parser check,
  bench-only: per saved session of the run, the ledger's uncached input, cache read, cache write and output against
  `tokscale models --json --group-by client,session,model` (omp's saved folders passed as `TOKSCALE_EXTRA_DIRS`, in a
  throwaway `TOKSCALE_CONFIG_DIR`; Claude Code's and opencode's records where they live in the VM). Tokens must match
  exactly; dollars aren't compared. tokscale is pinned at v4.21.0 (2026-11-02, MIT), the Windows binary from
  `https://github.com/junhoyeo/tokscale/releases/tag/v4.21.0`, approved for the VM only and not installed on his PC:
  without it the script stops and `test/bench/tokscale-parity.test.ts`'s live case skips, saying so. Writes
  `tokscale-parity.jsonl` and `tokscale-parity.json`; exits 1 on a mismatch.
- **H9, `bun bench/h9-footprint.ts --out <dir> --install "<command>" --uninstall "<command>" [--between "<command>"]
  [--roots <dir,…>]`.** Snapshots files and the registry before install, after install, after `--between` (0481's
  setup, first run and a thread, when given) and after uninstall; reports what install added, what setup and use
  added, and what uninstall left (all, and what names jocasta, herdr, omp or mise). Prerequisites and minutes to the
  first task go in the run's notes by hand. Writes `h9-footprint.json`, `h9-install-added.jsonl`,
  `h9-use-added.jsonl` and `h9-uninstall-left.jsonl`.

## The task benchmark

**`bun evals/bench.ts --tasks <tasks.jsonl> --cache <dir> --out <runDir> [--study main|followup]
[--arms a0,a1,a2,a3,a4,a6,a7] [--reps 3] [--set main] [--only id,id] [--pilot] [--workers 2] [--seed 482]
[--timeout 60] [--profile <omp profile>] [--work <dir>] [--pty-session bench-pty] [--validate] [--dry-run]`** runs the 40 curated tasks of
`D:/thesis/data/bench/tasks/tasks.jsonl` through the eval runner's `runTrial` (same transcripts, usage and cost, line
counts, findings and clean-clone hidden grading as `bun run evals`). The main run (0543) is 30 tasks × 3 reps × 7 arms
= 630 trials: A0 omp alone, A1 + the pane config, A2 + plan first, A3 + the review, A4 Claude Code, **A6** = A3 + the
harness-OS layer, **A7** = opencode bare. Results go to `D:/thesis/data/bench/tasks/runs/<run>/`: `results.jsonl`
(the runner's row plus `task`, `arm`, `language`, `attempt`, `trialIndex`, `leak` and `ledger`, the trial's ledger
totals: tokens, priced cost, cache read share, waste counts, peak context), `ledger.jsonl` (every counted trial's
request and tool-call rows), `spend.jsonl` (each attempt's priced cost, infra errors included), `sessions/<task>-<arm>-<rep>/`
(the harness's own records: omp's session files, written with `--session-dir` in the trial root; Claude Code's
transcript and subagent transcripts, found by the stream's `session_id` under `~/.claude/projects` or
`CLAUDE_CONFIG_DIR`; opencode's `opencode export <session>` of each session its stream names),
`transcripts/<task>-<arm>-<rep>.jsonl`, `logs/<task>-<arm>-<rep>/setup.log` and `grade.log` (each command, its folder,
exit code, seconds, stdout and stderr; a retried trial's attempts append) and `lines.log` (the per-file
`git diff --numstat` behind the row's line counts, 0508), `errors.jsonl` (infra errors), `conditions.json` (seed, trial
order, and per session start, end, OS, CPU, RAM, the versions of omp, claude, opencode, git, bun, node, python and go,
the work root, git's `core.longpaths`, Windows' `LongPathsEnabled`, the pacer's pauses and A7's priced spend) and, with
`--validate`, `validation.jsonl` with each task's `validation/<task>/setup.log`, `null.log` and
`gold-1.log`…`gold-3.log`. Real trials need `JOCASTA_EVALS_REAL=1`; `--dry-run` and `--validate` call no model. Exits
1 when a trial or task is left failing, 2 on a bad flag, 3 after three sign-in failures in a row, 4 when A7's spend
limit stopped its trials. The fixed choices:

- **A6 (0543, 0567):** `a6.yaml` extends A3 with `layer` (the code index switch; the `check` thread tool is always on, 0571), and plans
  first as Jocasta's threads do: not A3's `--plan-yolo`, but omp's interactive plan review answered "Approve and
  execute" (`plan_review: execute`); `modelRoles.default` and `.plan` are pinned to `anthropic/claude-opus-5-5:high`, so
  the review's planning role and execution tier stay on the arms' model. The layer is Jocasta's build, not the
  bench's: each A6 trial imports `src/frontier/layer/bench.ts` (`LAYER_MODULE` in `evals/layer.ts`), whose
  `benchLayer.setUp(trial)` returns the omp flags, overlay settings and environment that bring in the task file's brief,
  `report_result`, the cached checks and the index, and a `close()`. Until that module exists, a real run with A6 stops
  before any model call ("A6 needs Jocasta's layer build"), and the dry run marks the arm not runnable.
- **The plan review (0567, `evals/plan-review.ts`):** omp 18.18.2 offers "Approve and execute" (the session is cleared
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
- **Cache warming (0567):** every omp arm sets `providers.cacheWarming: streaming` (A0 is omp's defaults except this);
  only `f-keepalive` sets `idle`.
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
- **Per-trial tools:** Python tasks run in a venv at the trial root (`<root>/venv`, made once and reused by grading's
  setup, pip upgraded first), `venv/Scripts` first on PATH for setup, harness and grading; Node and Go setups run as
  they are (their caches are shared safely).
- **Order:** tasks × arms × reps shuffled with a seeded PRNG (seed 482), so arms interleave over the hours; `--pilot`
  is 5 tasks (seeded, 2 Python, 2 TypeScript/JavaScript, 1 Go) × 1 rep × the arms (35 trials with all seven arms).
- **Workers and pacing:** `--workers N` trials at once, each in its own worker thread; the quota pacer is awaited before
  each. It reads `omp usage --json` and waits only while every signed-in Anthropic account has its 5-hour or 7-day
  window at 85% or more (0543), until the first of them frees up; accounts are named `anthropic#1`, `anthropic#2` in
  the run's records. Never two trials of one task at once: a free worker takes the next trial in the seeded order whose
  task isn't running (hatch's and poetry's shared folders, npm/yarn/pnpm caches per project), so the order holds
  otherwise. A rerun with the same `--out` skips trials already in `results.jsonl`; a different plan (seed, arms, reps
  or tasks) in the same folder is refused.
- **Trial roots:** each trial's root (overlay, `repo/`, `venv/`, the grading clone) is a short folder, `t` and 6
  characters, in `--work` (default the OS temp folder); in the VM a short root such as `C:\t` keeps pnpm/yarn trees
  and Python under 260 characters. Nothing in the root names the task (0508: agents list it); the trial's `setup.log`
  names the root.
- **Trial repos:** the fixture is the base commit's tree, committed whole (files its own ignore rules match too, as
  upstream tracks them) and tagged with upstream's nearest tag at the base commit (found once at preparation from a
  fetch of commits and tags only), so tools that read the version from git report upstream's, as in a full clone. The
  setup runs in that checkout and what it writes joins the fixture commit; hooks it installs stay off
  (`core.hooksPath` points at a folder that doesn't exist), so an arm that commits meets no hook another arm never does.
  The trial branch is `eval/trial-<rep>`: a task's id ends in its fix's PR number, and git output shows the branch
  (0508: the pilot's jest-16439 A0 cited the PR from it). The full run passes `--timeout 90` (0508).
- **Infra errors are not task failures:** a failed setup, a sign-in failure (401/403, "not logged in", OAuth or token
  refresh), a rate limit or overload without a tool call, a rate limit at the end of a trial while the pacer saw a full
  account (0543), or a harness that ends before its first model response goes to `errors.jsonl` and is retried later,
  twice more at most. Three sign-in failures in a row stop the run (exit 3) naming the harness: sign in again and
  rerun to resume.
- **Leak scan:** each transcript (the prompt removed) is searched for the PR's and issues' URLs,
  `github.com/<owner>/<repo>/pull/<n>`, `/commit/<merge_commit>` and its first 10 hex; matches go in `leak`. The run
  never stops for one; the analysis drops leaked trials in a sensitivity check.
- **Validation (`--validate`):** per task, no model: the fixture with its setup graded unchanged on a clean clone
  (must fail), then `gold.patch` applied and graded on a fresh clean clone 3 times (must pass each time). Tasks that
  fail are listed for a spare of the same language.
- **Dry run:** the trial count, the first 15 trials of the order, each arm's harness command line, A6's layer switches
  and whether the layer build is there, A7's key, machine and limit, the pacer's rule, the ledger's files, and the
  cache and results paths; the main run's dry run also names the follow-up study's size.

**The follow-up study (0543, 0545, 0571), `--study followup`,** runs after the main run, in its own `--out`: 10 main
tasks (seeded, Python, TypeScript/JavaScript and Go in the main set's proportions) × 2 reps × A6 and five arms, 120
trials, each arm paired with A6 by task and rep (A6 runs again in the same order, so the pairs share the hours and the
build): `f-index-off` (the code index off), `f-prewalk` (`omp --prewalk`, present in 18.18.2), `f-keepalive`
(`providers.cacheWarming: idle`), `f-keep-context` (the review answered "Approve and keep context") and
`f-ponytail-if` (Ponytail with its "Running tests" rules inside `<important if="you are running or writing tests">`,
`evals/variants/ponytail-important-if.md`). pitchfork is no comparison (0560), and neither is the `check` tool: the
fixtures have no mise check, so A6's tool answers "No checks" and cached checks are measured in H8 (0571). `--arms`,
`--reps`, `--set`, `--only` and `--pilot` are refused with it. **The keep rule** (0543, written before any run),
`bun evals/keep.ts --out <follow-up runDir>`: a tried feature stays when its paired cost per passing task (the ledger's
priced cost summed over the pairs ÷ hidden-test passes) is lower and its pass rate isn't significantly lower (McNemar's
exact test on the paired trials, Holm's correction over the five comparisons, α = 0.05); for `f-index-off` the feature
is A6's side.
Writes `keep.json`.

The single cases of `evals/cases/`:

`bun run evals` (`evals/`) runs the arms: `--variant a0` … `a4` (A0 omp alone, A1 + the pane config, A2 + plan
first, A3 + the review turns, A4 Claude Code), `--profile <name>` for an omp profile apart from his daily one (log it
in once), hidden graders (`hidden_checks` and a `hidden/` folder per case), and the same quota pacer (it waits while
every Anthropic account has its 5-hour or 7-day window at 85% or more). `bun run evals compare <A> <B>` adds the paired
statistics (task bootstrap, pass^3, McNemar, Wilcoxon on wall time, cost and tokens, Holm). A4's model id and
permission flag are Claude Code's documented ones; confirm them against `claude --help` in the VM. A5:
`bun evals/a5.ts record --out <dir>` records the service's state feed (`/ws`) during his sessions, and
`bun evals/a5.ts report` counts interventions, attention minutes and the batch time from it and the session files.
