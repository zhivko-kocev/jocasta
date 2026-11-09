# 0554 herdr's keys, pings and settings for Jocasta

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V4, V11
- Date: 2026-11-09

## Question
With Jocasta a herdr plugin (0553): which keys run the thread actions, how does he hear about a thread that needs him, and which of herdr's settings change?

## Facts
Verified 2026-11-09 on herdr 0.9.5's default config.
- herdr's prefix is Ctrl+B; `prefix+n` is next tab, `prefix+s` settings, `prefix+r` resize mode; `prefix+alt+…` chords are unbound.
- `ui.toast.delivery = "system"` makes `notification.show` ask Windows' notification service, through the foreground client; `--sound request` plays the needs-attention sound when `[ui.sound]` is on. Jocasta's template has delivery off and sound off (0407).
- `experimental.pane_history` keeps recent pane screens across a full server restart (supported on Windows); `ui.status_indicators = "symbols"` draws distinct glyphs; `ui.window_title` sets the terminal's title from `{workspace}`; `tab_bar_right` holds Jocasta's HUD (0403, 0437).

## Options
| Item | Options (pros / cons) |
|---|---|
| Thread keys | Alt+Shift+F/S/R/N (one chord) · Prefix keys (herdr's style, fewer clashes) |
| Prefix letters | Ctrl+B, then Alt+F/S/R/N (all free) · Ctrl+B, then F/S/R/N, moving herdr's next tab, settings and resize · Other free letters |
| Pings | Windows notification with a sound on blocked · Silent |
| Settings | Screen history · Glyphs · Window title · herdr colours from `palette.ts` · HUD trimmed · pwsh as the shell |

## Recommendation
Alt+Shift chords; Ctrl+B then Alt+letters if prefix; sound on blocked; every setting.

## Choice
- Keys: Ctrl+B, then Alt+F Finish, Alt+S Stop, Alt+R Review, Alt+N New thread, each a `plugin_action` binding to 0553's actions. Alt digits, Alt+G and Alt+[ / Alt+] stay (0408).
- Pings: `ui.toast.delivery = "system"`; the service sends `notification.show` with `--sound request` when a thread needs him, and finished threads ping silently; `[ui.sound]` is on for that.
- Settings: `experimental.pane_history = true`; `ui.status_indicators = "symbols"`; `ui.window_title = "Jocasta · {workspace}"`; the HUD keeps the blocked count, spend and the time, while quota moves into `today`.
- Unchanged: herdr's `tokyo-night` without custom colours, and the default shell.

## Reason
"Prefix keys", then "Ctrl+B, then Alt+F/S/R/N"; "Windows notification + sound on blocked"; settings ticked: screen history, glyphs, window title, HUD trimmed.

## Consequences
- Differs from the recommendation on the keys (prefix) and on two settings (colours, shell).
- Amends 0407 (toasts off in herdr: herdr now delivers the service's notifications to Windows), 0403, 0437 and 0456 (the HUD's content) and 0408 (new prefix chords).
- Amended by 0568: Merge, PR and Stop everything take the typed code. Amended by 0569: finished threads chime with herdr's done sound. Added in the build: Ctrl+B then Alt+Q quits (asks the close question).
