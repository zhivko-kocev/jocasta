# 0697 Personal overlays

- Status: v1 (built 2026-11-26)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J4
- Date: 2026-11-26
- Amends: 0424 and 0450 (the launch order: the guard comes last), 0650 (a note: the shipped roles stay his)

## Question
The shipped snapshot carries his model roles (0590, 0650): Anthropic models. Someone else, or he on another provider, gets omp's fallback to an available model, and every `jocasta setup` writes the snapshot back over a change made in `~/.omp/agent/config.yml`. Where does a person set roles and other omp values durably for Jocasta's panes?

## Facts
- The boss starts as `omp --continue --config <config>/boss.yml --config <config>/guard.yml -e <bridge>`; a project or thread pane as `omp --continue [--append-system-prompt <prompt>] --config <config>/guard.yml -e <bridge> --config <data>/projects/<pid>/omp.yml`, then `heavy.yml`, `review.yml` and `--max-cost` for a thread.
- omp: a later `--config` overlay overrides an earlier one, and `bash.patterns` entries of a later overlay are prepended ahead of earlier ones (`omp://settings.md`). So the overlay loaded last decides both the scalars and the first rules.
- When a configured model is unavailable omp falls back to an available one.

## Options
| Option | Pros | Cons |
|---|---|---|
| Two optional files in `~/.jocasta/agent/`, read after the shipped overlays and before the guard, which moves to the end of every pane's launch | Survive setup; one place for the boss, one for every pane; the guard still wins | Two more files to know about |
| Ship no roles at all | Nothing to override | His own setup loses its roles |

## Recommendation
Two personal files; the guard last.

## Choice
His answers of 2026-11-26 and the approved plan:
- When `~/.jocasta/agent/boss.yml` exists, the boss launch gets `--config` for it between `config/boss.yml` and `config/guard.yml`.
- When `~/.jocasta/agent/pane.yml` exists, every project, thread, heavy and review pane gets `--config` for it immediately before `config/guard.yml`, after the project's overlay, `heavy.yml` and `review.yml`.
- The guard always comes last: in panes it moves from before the project's overlay to the end, so its values override every overlay and its rules come first. The weekly review's headless omp loads it last too.
- The shipped roles stay his (note on 0650). README says: the shipped roles are Anthropic models; omp falls back to your default model with another provider; these two files are where you set your own.

## Reason
People keep their own models without fighting setup, and nothing they write there outranks the guard.

## Consequences
- `pane_launch.rs` builds a pane's argv as program, `--continue`, prompt, bridge extension, overlays, then the personal file and the guard (`pane_tail`); `thread_start.rs`, `weekly_review.rs` and `boss.rs` follow.
- The files are read at each launch: a change applies to panes started after it.
