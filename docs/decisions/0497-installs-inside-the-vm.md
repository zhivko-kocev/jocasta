# 0497 What the agent installs inside the VM

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-27

## Question
May the agent install a new user's prerequisites, the benchmark's tools and R inside 0481's VM? Nothing goes on his PC.

## Facts
Verified 2026-10-27.
- Installs need his yes (AGENTS.md; 0385, 0435, 0476 and 0481 list what was approved so far).
- 0481's VM `jocasta-clean`: Windows 11 Enterprise Evaluation 10.0.26300, 4 virtual CPUs, 8 GB, user `tester`. The agent drives it over SSH with a key that works only from his PC, because Hyper-V's PowerShell Direct needs admin rights the agent's session doesn't have.
- A new user needs Git for Windows, Bun 1.4.3, omp and herdr 0.9.5 (`README.md`).
- The task benchmark (0482): Claude Code 2.1.296 for arm A4; the 30 tasks are Python, JavaScript/TypeScript and Go projects (`D:/thesis/data/bench/tasks/tasks.jsonl`), so Node 24 with npm, pnpm and yarn, Python 3.13 and Go 1.25; the repo's pinned packages; each task project's own dependencies.
- The analysis's mixed-effects logistic regression (0482): R's `lme4` is the standard package.

## Options
| Option | Pros | Cons |
|---|---|---|
| Yes, all of it, inside the VM only | The run starts without waiting on him | — |
| Ask before each one | He sees every install | A wait at each step |

## Recommendation
Yes, all of it, inside the VM only.

## Choice
Inside the VM only: Git for Windows, Bun 1.4.3, omp, herdr 0.9.5, Claude Code 2.1.296, Node 24 with npm, pnpm and yarn, Python 3.13, Go 1.25, the repo's pinned packages, each task project's dependencies, and R with `lme4`. Nothing is installed on his PC.

## Reason
"Yes, all of it, inside the VM only".

## Consequences
- AGENTS.md's install rule lists them.
- The question named omp 18.12.6; the VM got the version current that day, 18.13.1 (`bun install -g @oh-my-pi/pi-coding-agent@18.13.1`, as a new user's install would), and 0500 makes it the benchmarks' version.
- opencode isn't in the list: 0503 adds it.
- Amended by 0505: PowerShell 7 is installed in the VM too, for H1's `pwsh -c` lines.
