# 0720 Benchmark on Linux

- Status: v1 (built 2026-11-27)
- Kind: process
- Phase/slice: ultron 0.3 iteration, M3 B1
- Date: 2026-11-27
- Amends: 0482 (where the scripts run), 0657 (the table of places), 0674 (footprint's `--out`)

## Question
The thesis measures Jocasta on two machines: his PC, now on Linux, and his Windows 11 ThinkPad E14. Which of the benchmarks run on Linux, how does a Linux run read the peak memory Windows reads from a Job Object, and how does each result file say which machine, omp and Jocasta it measured?

## Facts
- The settings trials' watcher (`evals/watch.rs`) reads a run's peak from its Job Object on Windows and reads nothing elsewhere; its sampler already ticks every second for the memory floor.
- Every benchmark's summary carries `conditions` from one helper (`bench/lib/output.rs` `conditions`, 0482 [S16]): time, OS, kernel release, CPU, memory and the tools' versions; the task benchmark's `conditions.json` has one such block per session. None names the graphics card, the OS by name or the Jocasta commit. `footprint` (0674) prints its numbers and writes only the budget file.
- On Linux the kernel lists a thread's children in `/proc/<pid>/task/<tid>/children` and a process's resident memory in `/proc/<pid>/status` (`VmRSS`, kB).
- `h9-footprint` refuses off the VM `JOCASTA-CLEAN` and snapshots the Windows registry; `h1-guard git-guard-check --run` runs in `C:\work\demo` on that VM only (0657).
- The PC is his live desktop: a benchmark must not show notifications or windows there. H2 measures the time to his notification, which on Linux is `notify-send` to the desktop's notification server.
- `tasks.jsonl` (the task benchmark's 40 tasks) is on his Windows PC only, not yet on this PC.

## Options
| Option | Pros | Cons |
|---|---|---|
| Linux peak by sampling `VmRSS` over the tree every second; one machine stamp in `conditions`; a "Two machines" runbook | Same files and scripts on both machines; each result says where it ran; small change | RSS is not Windows' committed memory, and a 1 s sampler misses shorter spikes, so the two OSes' peaks are compared with that said |
| A Linux cgroup per run (`memory.peak`) | Exact peak, like the Job Object | Needs systemd-run scopes or delegation; more code; not on every Linux |

## Recommendation
Sampling, the stamp and the runbook.

## Choice
His answers of 2026-11-26 and the approved plan (B1):
- `evals/watch.rs`: on Linux `peak_tree_bytes` is the largest sum of `VmRSS` over the run's process tree (the root and every descendant through each thread's `children` file) that the 1-second sampler saw; Windows keeps the Job Object's peak; macOS reads none.
- Every benchmark's `conditions` gains `machine {os, kernel, cpu, memoryGb, gpu}`, `ompVersion` and `jocastaCommit` (the commit of the checkout the bench runs from, `-dirty` when a tracked file differs), read by the one helper every script and the task benchmark use. The OS is `/etc/os-release`'s `PRETTY_NAME`, Windows' edition caption, or `sw_vers`; the graphics cards come from `lspci`, `Win32_VideoController` or `system_profiler`.
- `jocasta-bench footprint --out <dir>` also writes `<dir>/footprint.json` (conditions, each run, the medians); the budget check is unchanged.
- Results go to `~/thesis/data/linux-pc/<date>/<bench>` on the PC and `D:\thesis\data\win-laptop\<date>\<bench>` on the laptop, each bench in its own folder, uncommitted, for him.
- Windows only, said so in `bench/README.md`: `h9-footprint` and `h1-guard git-guard-check` (both VM-bound). Runs on the laptop, not on the PC: `h2-toast`, because its notifications would show on his live desktop.
- A Linux run's sandbox: `JOCASTA_DATA_DIR`, `JOCASTA_CODING_AGENT_DIR` and `PI_CODING_AGENT_DIR` under `/tmp`, the sandbox Jocasta in a detached tmux session, and no display or session bus in its environment (`WAYLAND_DISPLAY`, `DISPLAY` and `DBUS_SESSION_BUS_ADDRESS` unset; `notify-send` and `xdg-open` replaced by stubs that log), so nothing reaches his desktop.

## Reason
Not given; the plan's B1.

## Consequences
- `watch.rs` gains the `/proc` reader with unit tests; `output.rs` gains `Machine`, `machine()`, `os_release_name`, `lspci_gpus` and `repo_commit` with unit tests.
- `bench/README.md` gains "Two machines": the Linux commands that ran and the PowerShell lines for the laptop, the task benchmark's command for when `tasks.jsonl` is copied over, and how results come back into `~/thesis`.
- A peak or memory figure from Linux is RSS, from Windows committed or working-set memory; the thesis compares them with that said.
- The footprint budget is per OS (2026-11-27): `bench/footprint-budget.<os>.json`, read and ratcheted for the OS the bench runs on. The Linux build is 27.6 MB against the 21.8 MB budget Windows set before the native views; the 6 MB more is iced (0721), his decision, so it isn't a regression to ratchet against. `footprint-budget.linux.json` is the Linux run's own worst numbers; `footprint-budget.windows.json` is the old file kept as it was, to be measured again on the laptop with `--write-budget` (the Windows build carries iced too).
