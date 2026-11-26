# 0696 Snapshot: local commit model, no sleep inhibitor on Linux

- Status: v1 (built 2026-11-26)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J3 (#9, #12)
- Date: 2026-11-26
- Amends: 0590 (the commit role), 0650 and 0651 (one Linux value in the snapshot)

## Question
Every commit message costs a cloud call even on a machine with a local model running (issue #12), and on Linux every omp Jocasta runs holds a logind idle inhibitor, so swayidle turns off auto-lock and screen-off while Jocasta runs (issue #9). What does the omp snapshot ship?

## Facts
- `config/omp/config.yml` had `modelRoles.commit: anthropic/claude-haiku-5-5:low`. omp's role values are comma-separated selectors, the first available wins; its `ollama` provider needs no key at `127.0.0.1:11434`.
- The snapshot had no `power` key, so omp's default `power.sleepPrevention: idle` applied; omp 19.2.1 offers `off|idle|display|system`. In the VM, swayidle 1.9 logged "Not enabling timeouts: idle inhibitor found" until it was set to `off`.
- Setup already adds per-OS values to the snapshot's `config.yml` (Windows' and Linux's fenced folders, 0594, 0701), shown in its diff.

## Options
| Option | Pros | Cons |
|---|---|---|
| Local-first commit role; `power.sleepPrevention: "off"` added by setup on Linux | Commit messages free where Ollama runs, Haiku elsewhere; idle timers work while Jocasta runs | A Linux machine can sleep during a long unattended turn unless its own idle settings say otherwise |
| Leave both as they were | No change | Paid commit messages; no auto-lock on Linux |

## Recommendation
Local-first commit role; sleep prevention off on Linux.

## Choice
His answers of 2026-11-26 and the approved plan:
- `modelRoles.commit: ollama/qwen3:4b, anthropic/claude-haiku-5-5:low` in the shipped snapshot, on every OS.
- On Linux setup adds `power.sleepPrevention: "off"` to the snapshot's `config.yml`, shown in setup's diff like the Linux fence. Windows and macOS keep omp's default.

## Reason
Spend less: a commit message needs no cloud model; and Jocasta must not switch off his desktop's idle lock.

## Consequences
- An existing install sees both as snapshot changes in `jocasta setup`.
- A machine without Ollama, or without `qwen3:4b` pulled, keeps Haiku.
- ultron's `set_omp_settings` no longer needs to set either for a Jocasta boss (U10).
