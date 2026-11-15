# 0623 omp 18.20.2 everywhere; its selective-commit text stays on

- Status: v1
- Kind: product, benchmark
- Phase/slice: program
- Date: 2026-11-15

## Question
omp on his PC became 18.20.2 during the settings trials, against 0600's 18.20.0 everywhere. Which version do Jocasta and the benchmarks run on, and does omp's new selective-commit text stay in Jocasta's panes?

## Facts
Verified 2026-11-15 (the file lists are in `D:/jocasta-spikes/sota/omp-18.20.2/`, outside the repo).
- **What happened:** at 00:50Z omp's updater renamed `~/.bun/bin/omp.exe`, bun's shim, to a `.bak` and wrote 18.20.2's standalone binary in its place; bun's `omp.bunx` is not there now. omp's `cli/update-cli.ts` does this in two cases: bun ran, exited without error and the launcher still reported the old version, or `omp.bunx` was already missing and the shim was taken for omp's own binary. Which of the two it was is not known. He started the update himself (his answer of the same day, 0624). Bun's global package, whose source H1's guard tests load, stayed 18.20.0, untouched since 2026-11-13: `omp --version` says 18.20.2 and those tests read 18.20.0.
- **The difference** (the 18.20.0 package's `src/` against 18.20.2's npm tarball, read and not installed): 184 of 1,962 files differ and 12 are new. Of the 60 omp source files that the plan and Jocasta's code cite, 20 changed.
- **For Jocasta, read from that diff:**
  - `bash.patterns` and `bashInterceptor.patterns` merge across config layers, the higher layer's entries first; before, a higher layer replaced the list. The first matching rule still decides, so the guard's rules come before any in his own config. His config has none.
  - `grep.contextBefore` and `grep.contextAfter` default to 0 (they were 1 and 3).
  - New, on by default: `commit.enabled` (a `commit` part in the eval tool's text, and `/commit`: an agent commits only the lines it and its subagents wrote) and `todo.midRunNudges: capped` (a reminder after 12 edits or commands, at most twice).
  - New: the `session_clear` extension event and the flags `--no-judge`, `--no-spotlight` and `--offline`.
  - Unchanged: `advisor/settings.ts`, `task/types.ts` (the depth rule of 0619) and `session/acp-permission-gate.ts` are identical. `task/executor.ts` and `task/index.ts` changed, but the depth check in them is the same expression. `session_stop` in `extensibility/shared-events.ts` is as it was.
- **Fixed tokens on 18.20.2** (no model call): omp's defaults 10,161 (10,013 on 18.20.0); a Jocasta pane 11,022 (10,874). `commit.enabled: false` takes 151 off a pane; `todo.midRunNudges: off` and the old grep context take 0. 0619's differences are equal on both versions (−41, −1,495, −939).
- **The trials of 0619 and 0620:** 131 of the 144 ran on 18.20.0 and 13 on 18.20.2; the 6 `advisor.syncBacklog` trials ran on 18.20.2. All passed. Arms are compared within the same case and rep, and each such group ran on one version.
- **Not checked on 18.20.2:** H1's resolver tests (they need the package's source at that version), plan 5.5's full list, the VM.
- **0383:** omp is his daily install, and a bump re-checks plan 5.4's seams and 5.5's list.

## Options
| omp version | For | Against |
|---|---|---|
| Move the pin to 18.20.2 | the benchmarks measure the omp he uses; smaller grep results | one more slice before the others; 91% of the trials ran on 18.20.0 |
| Stay on 18.20.0 and put his PC back | nothing to re-check; every number so far is on it | he holds omp back until the benchmarks end |
| 18.20.0 in the VM, 18.20.2 on his PC | no work now | his PC runs an omp that Jocasta's seams and guard were not checked against |

| Selective-commit text | For | Against |
|---|---|---|
| Keep omp's default, on | in a project pane the agent shares the checkout with his own edits, and its commit leaves his lines out | 151 fixed tokens in every pane and subagent session; its value was not measured |
| Off in Jocasta's panes | 151 tokens less a session | agents commit with plain git, as on 18.20.0 |

## Recommendation
Move the pin to 18.20.2; keep the text on.

## Choice
- **omp 18.20.2 is the pinned version everywhere:** his PC, the VM and every benchmark arm.
- **The bump is the first build slice,** held with the others (0618): bun's global package to 18.20.2, an install this answer approves; `PINNED_OMP`, the VM's install line and the "checked against" cites; plan 5.4's seams and 5.5's list re-checked against 18.20.2's source, the 20 changed files first; H1's resolver tests on that source.
- **`commit.enabled` stays omp's default, on, in every pane.** `todo.midRunNudges` and the grep context stay omp's defaults too.

## Reason
His answers: `pin-18.20.2` and `keep-on`.

## Consequences
- Amends 0600 (the pinned version).
- 0619's and 0620's decisions stand: the settings' differences were measured again and are equal on both versions, and the advisor's settings and the depth check are the same code.
- The baseline was taken on omp 18.20.0 and earlier, so the before-and-after measurement names the omp version on each side; the smaller grep results are omp's gain, not a slice's.
- A seam the re-check finds broken stops that slice (0020).
- Plan appendix I's omp line moves to 18.20.2 with the build (0027).
