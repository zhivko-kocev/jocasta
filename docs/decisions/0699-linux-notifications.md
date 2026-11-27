# 0699 Linux notifications

- Status: v1 (built 2026-11-26)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J1 (#4, #5, #13)
- Date: 2026-11-26
- Amends: 0407 (one place for notifications), 0578 (questions are logged), 0678 (a brief opens from its notification)

## Question
On Linux a blocked thread or an Allow question arrives as an ordinary notification that times out, questions leave no line in `notifications.jsonl` (so a desktop that follows the log for `blocked` never sees the one notification that expires), and a Done notification can't open the thread's brief. What do Linux notifications do?

## Facts
- `own_surface.rs` ran `notify-send --app-name=Jocasta -- <title> <body>` for every note within a 10 s bound; `Sound::Request` was used only on Windows.
- Question notes went from `native.rs` straight to the surface, never through `Events::after_notify`, which alone writes `notifications.jsonl` (issue #4).
- `Toast.url` holds a Done toast's brief address; `Note` had no link (issue #13). `notify-send -A NAME=Label --wait` prints the action's name when he clicks it.
- The freedesktop sound naming spec's `message-new-instant` is the sound of a message that wants an answer.

## Options
| Option | Pros | Cons |
|---|---|---|
| Critical urgency and the sound hint for notes that need him; questions logged as `blocked`; Done offers "Open" and waits for the click | Windows parity for the request sound; the log is complete; the brief one click away | A waiting `notify-send` per Done note while the notification is shown |
| Also ring the terminal bell for an urgency hint | Window managers mark the terminal urgent | Another path; the plan didn't take it |

## Recommendation
The first.

## Choice
His answers of 2026-11-26 and the approved plan:
- A note with `Sound::Request` (blocked threads, urgent toasts, every question) runs `notify-send` with `-u critical --hint=string:sound-name:message-new-instant`.
- A question's notification is a blocked toast (`Jocasta asks — <question>`) sent through `Events::notify`, the toasts' own path (`NativeServer::notify` takes a `Toast`), so it is raised with the request sound and logged as one `notifications.jsonl` line with `reason: "blocked"` and title "Jocasta asks".
- `Note` gains `link` (`NoteLink {url, open}`), filled for a Done toast whose `url` is a brief Jocasta stored. On Linux such a note runs `notify-send -A default=Open --wait …` on its own task (up to an hour, not the 10 s bound); when it prints `default` the brief opens with a fresh launch code, as the dock's Enter does. Windows and macOS keep their notifications as they were.

## Reason
The Allow is the notification most likely to be missed and the only one that expires; a finished thread's result should be one click away.

## Consequences
- `own_surface.rs` builds `notify-send`'s argv in one function; `events.rs` writes every line through one writer; `server.rs` sends question notes through the service.
- ultron's attention daemon can drop its D-Bus reader of question toasts once this ships.

## Note (2026-11-27)
In the VM three "Jocasta asks" notifications stayed on screen after three Deny answers: critical notifications never time out, and nothing closed them. A question's toast now carries what ends it (`Toast::until`, the popup's end); on Linux its `notify-send` gets `-p`, which prints the notification's id, and once the question is answered, denied, withdrawn or closed Jocasta calls the freedesktop `CloseNotification` with that id (`gdbus call --session --dest org.freedesktop.Notifications --object-path /org/freedesktop/Notifications --method org.freedesktop.Notifications.CloseNotification <id>`; `gdbus` comes with GLib, which `notify-send` needs). Windows and macOS keep theirs as before: the toast stays in the notification centre.
