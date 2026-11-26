# 0700 `jocasta status --json`

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J5 (issue #14)
- Date: 2026-11-26

## Question
ultron's bar shows whether Jocasta runs and whether something needs him. Today it guesses from `pgrep`. What can a status bar or a script ask Jocasta for?

## Facts
- The running service already holds the HUD's counts (`today`'s Awaiting you rows, threads waiting on him, the UNDERWAY rows), today's spend as last counted and every open thread with its state.
- `jocasta thread …` reaches the service over its bridge pipe with the token in `<data>/jocasta-service.json` (0544, 0675) and exits 3 when Jocasta isn't running.
- ultron's bar calls it with `timeout 2` every third tick, so the answer must not wait on any slow read.

## Options
| Option | Pros | Cons |
|---|---|---|
| `jocasta status --json` over the bridge, from what the service holds | One JSON line; nothing parsed from session files on the request | One more command |
| Read `<data>/hud.txt` | No command | No threads, no "running", stale after a crash |

## Recommendation
`jocasta status --json`.

## Choice
His answers of 2026-11-26 and the approved plan:
- `jocasta status --json` prints one line: `{"running": true, "version": str, "needsYou": int, "blocked": int, "underway": int, "spendToday": number, "threads": [{"project", "title", "state", "needsYou": bool}]}` and exits 0.
- When Jocasta isn't running (no service file, or its bridge doesn't answer) it prints `{"running": false}` and exits 3. Any other failure exits 1 with a line on stderr.
- Builders' details: `needsYou` is the count of Awaiting you's rows (the HUD's); `blocked` the threads waiting on him; `underway` the UNDERWAY rows; `spendToday` the spend last counted, 0 until today's sessions were read; a thread's `state` is `working`, `blocked`, `done` or `idle`, and its `needsYou` says whether an Awaiting you row is its pane's. `--json` is the only form: `jocasta status` without it is a usage error (exit 2).

## Reason
A status bar learns everything it shows in one fast call, without guessing from process names.

## Consequences
- `Route::Status`; `thread_cli::run_status_json`; the service's `status json` bridge command answers from `Status::status_json`, which reads nothing but the day's queue.
- ultron's bar reads it every third tick instead of `pgrep`.
