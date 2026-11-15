# 0618 Settings are decided by trials; the builds wait until everything is settled

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-15

## Question
The tuning program had settings proposals left with no record, and three decided builds waiting. Who chooses the settings, on what, and when do the builds go on?

## Facts
Verified 2026-11-15.
- **Not asked: his direction**, given while a build ran. Two messages, 2026-11-15:
  - "more philosophy for the decisions before building.", with three texts (an AI software factory, ontologies as knowledge to compute over, an enterprise path to an autonomous SDLC) and two links (the Harness Playbook, Ousterhout's "Modular Design" notes).
  - "also i want for the settings fine tuning for you to try and see what is better and decide by that, before building we will change to opus 5.5 high after we have everything settled".
- **0005** makes defaults his decision; **0363** lets builders choose the details that carry out a decided record.
- **Open settings proposals without a record** that day: the advisor in headless runs and in the boss, the three always-on omp settings, the thinking level, the subagents' depth, the cache's retention, two compaction settings (`docs/research/state-of-the-art.md`, sections 3 and 9).
- **What can be tried here:** fixed tokens of any setting with no model call (`omp --mode rpc`, `omp toks`); counts over his own session files; real trials on the 12 cases of `evals/cases/`, which every arm passes, so they show cost and time and not quality.
- **One build was running** on a decided record (0617) when the second message came. Three decided builds had not started (0610's lint rules and ceilings, 0610's tree checks, 0614's knip cleanup).

## Options
| Option | For | Against |
|---|---|---|
| The trials decide a setting; he is told the measurement | his direction; no question without a number behind it | a trial can be too small to show a loss |
| Every setting is asked (0005 as it stands) | he sees each | he asked for the opposite |

## Recommendation
None was made: his direction.

## Choice
- **An omp setting in Jocasta's shipped files, the panes' overlay or a benchmark arm is tried and decided by the result.** Its record names the measurement. He is told what was decided and by which number, and can overrule.
- **What the trials here cannot decide is not decided by them:** a setting that trades quality goes to the benchmark as an arm, or is asked.
- **Decisions are held against the factory view first** (`docs/research/state-of-the-art.md`, section 14): what is his from it is asked before any build.
- **No build starts until the decisions are settled.** He then changes the model to Opus 5.5 at high, and the builds run on it.

## Reason
His two messages, quoted in the facts.

## Consequences
- Amends 0005 for omp settings in the tuning program: the trial decides and he is told. 0363 stays as it is.
- The build of 0617, already running on its record, was let finish; it can be reviewed again on Opus 5.5.
- The three held builds, and the slices decided after this record, start only on his word that the model is changed.
