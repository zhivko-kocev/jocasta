# 0718 The board

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, M3 W1
- Date: 2026-11-27
- Amends: 0717 (the key line's last keys gain `w Board`)

## Question
Workspace 4 is for watching the agents. What does Jocasta show there of its running threads, how does it stay current, and how does he learn of a thread's progress when he is away from the desk?

## Facts
- The dock's snapshot holds per open thread its title, project, the line of what it does now, its state and since when, its session's cost and context, its cost cap, its last check and its diff (`DayThread`, 0555, 0668, 0686); the check queue's load (0671) and today's spend (the HUD) sit beside it. A review thread's pull request has its CI state in the PR cache (0668).
- A thread's phase in the task cycle (`thread.rs` `Phase`: task, review, handoff, finish) and what `today` shows (`ThreadState`: working, blocked, done, idle) are both held by the service; `stuck` says when the loop cap or the cost cap stopped it (0677, 0686).
- The brief windows are native iced 0.14 windows with an app id ultron places, reaching the service over the bridge pipe as a CLI peer (0721). `status --json` answers over the same pipe (0700).
- `<data>/notifications.jsonl` logs every notification as `{at, paneId, reason, title, body, result}`; ultron's `attentiond` follows it (0699).
- Rust 1.100's `File::try_lock` takes an advisory lock on Linux, macOS and Windows, released by the OS when the process ends, crashed or not.

## Options
| Option | Pros | Cons |
|---|---|---|
| A native board window, polled over the bridge; phase lines in `notifications.jsonl` for the phone | One look with the other windows; no web server, token or tailnet surface; the phone hears of progress through ultron's existing push | Not viewable from another computer |
| The plan's `/board` page with a tailnet view (W1 as first written) | Viewable from any device on the tailnet | A web server, cookie, token and Host rules the native views just removed (0721) |

## Recommendation
The native board window with phase lines.

## Choice
His answers of 2026-11-26, the approved plan and his 2026-11-27 amendment ("native views", item 4):
- `jocasta board` and the dock's `w` open one board window, Linux app id `jocasta-board` (ultron puts it on workspace 4), titled `Jocasta board`. One at a time: the window holds a lock on `<data>/board.lock`; a second `jocasta board` while it is open prints `The board is already open.` and exits 0 (nothing is duplicated; focusing it is left to the desktop).
- Built by code from the dock's snapshot: one row per open thread (running, waiting or blocked), each with `<project>/<title>`, its phase, the line of what runs now, its age (since its last state change), cost against its budget (`$1.20 of $5`), context, the last check and its pull request's CI when it has one; then the starts waiting for their turn (0687), the check queue (`checks: 1 running, 2 waiting`) and today's spend. No buttons: it is for watching.
- A thread's phase, as the board and its lines say it: `stuck` (the loop or cost cap stopped it), `review`, `handoff`, `finishing` (its PR turn), then `done`, `plan ready`, `awaiting you`, `working`, `idle`.
- New bridge command `board json`: the board's data as JSON (`Board` in `protocol.rs`). The window asks it every 3 s and patches its rows in place by thread; without a running Jocasta it shows `Jocasta isn't running.` and keeps asking.
- Motion (0713's tokens): only the first open has an entrance (rows fade in and rise 4 px over `fast` 180 ms, 25 ms later per row up to the fifth); afterwards a new row fades in over 180 ms and a row whose phase changed lights its phase and fades the light out over `base` 220 ms. Nothing moves on a poll that changed nothing; `JOCASTA_REDUCED_MOTION=1` keeps opacity only.
- Phase lines: when an open thread's phase changes, the service appends to `<data>/notifications.jsonl` `{"at", "paneId", "reason": "phase", "title": "<project>/<title>: <phase> · $<spent>", "body": "", "result": "logged"}` and shows no notification. One line per change, checked on each refresh tick; the same phase again writes nothing; a thread first seen (a new start, or one found at the service's start) writes nothing until its phase changes. ultron turns the line into a phone push.
- The key line's always-keys become `n New · b Brief · w Board · X Stop all` (amends 0717).

## Reason
His amendment of 2026-11-27: no HTML anywhere; the remote view is a push of each phase change, not a page.

## Consequences
- New `src/frontier/server/board.rs` (the phase, `board json`, the phase lines on the refresh tick), `src/frontier/view/board.rs` (the window), `Route::Board`, `client::board`, `Board`/`BoardRow` in `protocol.rs`; `view::spawn` takes its arguments.
- W1's `/board` route, `/board.json`, `board.remoteHost`, the board token and cookie, `jocasta board --url` and ultron's `tailscale serve` are not built (amendment item 4).
- Plan appendix I: the command, the dock key, the bridge command, `<data>/board.lock` and the `phase` line of `notifications.jsonl`.
