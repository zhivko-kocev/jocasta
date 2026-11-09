# 0482 The benchmarks, pre-registered

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-24

## Question
The thesis must show where Jocasta is better or worse than other tools. How far do the benchmarks go?

## Facts
Verified 2026-10-24.
- OpenAI stopped reporting SWE-bench Verified on 2026-02-23 (contamination and flawed tests) and recommends SWE-bench Pro (https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/).
- SWE-bench-Live has a Windows split: 66 PowerShell tasks over 48 repos in Windows container images (https://huggingface.co/datasets/SWE-bench-Live/Windows); it is the only Windows-native public set.
- Terminal-Bench 4.0 (66 tasks, a flat 8-hour agent timeout) needs a GPU sandbox and isn't runnable on one Windows machine (https://www.tbench.ai/run).
- The harness matters: the edit tool alone moved Grok Code Fast 1 from 6.7% to 68.3% ("The Harness Problem", https://blog.can.ac/2026/02/12/the-harness-problem/), and omp ships that edit format.
- Evaluations must report accuracy and cost together ("AI Agents That Matter", https://arxiv.org/abs/2407.01502).
- The competitors on Windows and their defaults before a shell command are in `local://competitors.md`; on his machine `claude` 2.1.296, `opencode` 1.18.22, omp 18.12.6 and herdr 0.9.5 are installed.
- `evals/` is saturated: the omp baseline scored 36/36 for $2.40 in total (`docs/status.md:26`).
- Power: 30 tasks × 3 reps detect only differences of about 20 points or more (α = .05, power .8).
- The pilot's quota per trial is read from `omp usage --json`'s 5-hour and 7-day windows.

## Options
| Option | Pros | Cons |
|---|---|---|
| Harness benchmarks + a pilot, then decide | No model calls until the pilot shows the real quota cost | The task benchmark's size waits for a second decision |
| The full plan now | Every result the thesis needs, pre-registered at once | The most quota and his time (A5) |
| Harness benchmarks only | No model calls; fast | No task-success result; can't say where Jocasta wins or loses on real work |

## Recommendation
Harness benchmarks + a pilot, then decide.

## Choice
Pre-registration, written before any run:
- **Harness benchmarks without model calls, H1–H9** (procedures, counts and metrics as in `local://benchmark-design.md`):
  - H1: the guard's block rate on a 300-command corpus;
  - H2: time from "needs you" to toast;
  - H3: crash and restart recovery;
  - H4: memory isolation (canary facts);
  - H5: startup time;
  - H6: RAM per pane and total;
  - H7: long-path robustness;
  - H8: parallel isolation and overlap detection;
  - H9: install and uninstall footprint (in the 0481 VM).
- **Task benchmark:**
  - 30 tasks × 3 reps, same model and thinking level, arms A0–A4:
    - A0: omp alone;
    - A1: + Jocasta's pane config;
    - A2: + plan first;
    - A3: + the review step;
    - A4: Claude Code.
  - Plus A5, his case study: 10 real tasks end to end through the boss against omp alone.
  - Metrics: verified success on hidden tests regraded on a clean checkout, pass^3, wall time, quota and API-equivalent cost, tokens, lines changed, reviewer findings (A3); for A5, interventions, attention minutes and batch time.
  - Analysis:
    - a mixed-effects logistic regression and a paired task bootstrap;
    - McNemar's and Wilcoxon tests with Holm correction;
    - confidence intervals and effect sizes;
    - a "Where Jocasta loses" table reported before the wins.
- **Quota:**
  - Model calls use his subscriptions only: his Anthropic login for the agents and for Claude Code; the review step's reviewer stays on his Copilot login (0460). No API keys.
  - A run pauses when either quota window (5 h or 7 d, from `omp usage --json`) passes 85% and resumes after it resets.
  - A 25-trial pilot runs first, and its quota per trial and time are shown to him before the full run.
- **Tasks:** never client code. They come from public, Windows-runnable open-source repositories with issues closed after the model's training cutoff (SWE-bench-Live, including its Windows split when Windows containers work here; those runs switch Docker to Windows containers, pausing Graphiti), plus tasks he adds from his own non-client projects.
- **Pinned for the whole run:** omp 18.12.6, Claude Code 2.1.296, herdr 0.9.5.
- **Where things live:** the harness in the repo under `bench/`; fixtures and raw results (transcripts) outside the repo in `D:/thesis/data/`; aggregated results in the thesis.

## Reason
"the full plan but on the anthropic subscription".

## Consequences
- Differs from the recommendation: the full plan now, on his Anthropic subscription.
- This record is the pre-registration; a change to it before the run is a new record.
- H9 runs in 0481's VM.
- Amended by 0488: arm A3's reviewer runs on the arms' model (`anthropic/claude-opus-5-5`), a same-vendor review in a fresh context; no Copilot login is needed.
- Amended by 0500: the run uses omp 18.13.1; Claude Code 2.1.296 and herdr 0.9.5 stay pinned.
- Amended by 0501: H1's corpus is the 1,285 commands as built (224 collected, each as written and rewritten), scored overall and for each way of writing.
- Amended by 0508: after the pilot, the full run starts with trial branches named `eval/trial-<rep>` (no task id), a per-trial `lines.log`, and a 90-minute limit for every arm.
- Amended by 0516: in the VM's sandbox, the agent operates the benchmarks' own steps (Allow on a bench thread's box, closing threads in herdr between runs, Enter), each logged with its time; H8's plans are approved unread, as A2 does.
- Amended by 0517: Jocasta's benchmarks run on a build whose herdr client asks a dropped read again once; H2, H5 and H6 run again on it, then H8 and H3; the release's rows stay as a comparison.
- Amended by 0520: every Jocasta row runs again on the build with 0518 and 0519; H2 runs in front, minimized and behind the terminal, and its t3 is the moment Jocasta's toast window opens, read by a UI Automation watcher.
- Amended by 0522: H3 counts a herdr or pane fault recovered when every workspace that had an omp pane before it has one again; the herdr fault keeps the time until herdr lists the old pane ids as a second figure.
- Amended by 0523: H3's session check accepts a session entry anywhere in a file of JSON lines.
- Amended by 0528: H3's pane fault kills the omp in the folder it killed longest ago, at most once in 10 minutes per folder, so each kill meets 0521's first ending; each restart's client ends before the next fault; H3 runs again before the full run.
- Amended by 0529: H8 counts a thread finished when it is idle or done with a commit on its branch; the 8-thread run runs again before the full run.
- Amended by 0530: an H3 run after which Jocasta has closed itself counts not recovered, and H3 starts Jocasta again before the next run; H8's 8-thread run and H3 run with the agent watching, before the full run.
- Amended by 0533: every benchmark (H1–H9 and the task run) runs again on the harness-OS build of 0532, which becomes the build of record; the rows on `c154055b…004b` stay as the comparison.
- Amended by 0543: the rerun's task run has seven arms (A6 = A3 + the harness-OS layer, A7 = opencode bare); the layer's parts go to a follow-up study (10 tasks × 2 reps each); H8 gains a 4-thread run with the layer on and off; a tried feature stays when it is cheaper per passing task and not significantly worse; the pacer waits only while every account is at 85% or more.
- Amended by 0537: one Anthropic API key, with a $150 spend limit, for opencode's arm A7 only; every other arm stays on his subscriptions.
