# 0558 The herdr plugin works on Windows; a popup doesn't stop socket keys, so Allow takes a typed code

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V4, V11
- Date: 2026-11-09

## Question
0553's spike on this PC: do herdr plugins work on Windows, does the popup keep a thread from answering, and how does he answer an Allow question?

## Facts
Observed 2026-11-09 on herdr 0.9.5 and omp 18.18.2 in a throwaway session `jocasta-spike` with a throwaway plugin (`D:/jocasta-spikes/herdr-plugin/`).
- `herdr plugin link` works without a server; `plugin unlink` needs one. The startup hook ran at server start.
- A split plugin pane opens with `plugin pane open --placement split --direction right --target-pane <pane>`; herdr lists it with `agent_status: unknown` and `agent list` stays empty.
- A popup (`placement = "popup"` in the manifest; the CLI's `--placement` has no `popup`) opened with `--env SPIKE_TOKEN=…`, received the token, and had no pane id.
- `herdr pane send-keys w1:p1 enter`, aimed at an ordinary pane, reached the open popup, which logged `answer=allow`.
- `[[keys.command]] type = "plugin_action"` with `prefix+alt+f`, `prefix+f` and `alt+shift+f` each ran the action after `server reload-config`; one press before the reload didn't.
- `notification show … --sound request` with `ui.toast.delivery = "system"`: he saw the Windows notification with its sound.
- 0556 checked: F9 sent through `herdr pane send-keys` to an unfocused omp pane started recording; he spoke; the second F9 stopped it; the text was submitted by itself and omp answered.

## Options
| Option | Pros | Cons |
|---|---|---|
| Enter / Esc, risk accepted | One key | A thread can answer through herdr's socket (0552's accepted risk) |
| Type a 3-letter code to allow | Blind socket keys can't match a code they can't see | A few keystrokes per question; holds only if nothing can read the popup |

## Recommendation
Type a 3-letter code to allow.

## Choice
- The `ask` popup shows a random 3-letter code; typing it allows, Esc denies, and closing it any other way denies. Wrong letters don't allow.
- The build checks that `pane read`, `agent read` and `session.snapshot` can't read a popup's screen; if one can, the popup falls back to Enter / Esc with 0552's accepted risk.
- Everything else of 0553 and 0554 stands as the spike showed it.

## Reason
"the recomended", after the plain explanation that a thread could press Enter for him through herdr.

## Consequences
- Amends 0553 (the popup alone doesn't block socket keys; the code does) and 0519 (Enter is no longer the default for Allow).
- Left on disk: `D:/jocasta-spikes/herdr-plugin/`, `%APPDATA%/herdr/sessions/jocasta-spike/`, `%APPDATA%/herdr/plugins/config/jocasta.spike/` (empty). Nothing running; plugin unlinked; his `keybindings.yml` back to `app.live.toggle: []`.
