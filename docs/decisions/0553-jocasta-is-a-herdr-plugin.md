# 0553 Jocasta is a herdr plugin: `today` beside the boss, questions in a popup, after a spike

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V4, V11
- Date: 2026-11-09

## Question
He asked for "full herdr user experience exploration and come up with the full herdr setup/setting/ui" (`docs/research/herdr-experience.md`). How does Jocasta sit in herdr, where does the `today` pane go, and where do questions appear?

## Facts
Verified 2026-11-09 on herdr 0.9.5 (`herdr --default-config`, `herdr api schema --json`, the 0.9.5 docs).
- herdr plugins: a `herdr-plugin.toml` with actions (bindable to keys through `[[keys.command]] type = "plugin_action"`), event hooks, startup hooks, panes and link handlers; commands are argv, with `HERDR_BIN_PATH`, `HERDR_PLUGIN_STATE_DIR` and a context JSON injected; `herdr plugin link` registers a local directory. On Windows plugins are "preview … best-effort".
- Plugin panes open as `split`, `tab`, `overlay`, `zoomed` or `popup`; `plugin pane open --env` passes variables to the launched process. A popup is session-modal, "receives all terminal input, including Escape", closes when its command exits or on `popup.close`, and "has no pane ID … does not participate in pane, layout, persistence, or agent APIs".
- `agent.view.set` filters and sorts the Agents sidebar; plugins re-apply it from a startup hook.
- 0552 put the questions in the `jocasta` pane, accepting that a thread could answer through `pane.send_text`.

## Options
| Item | Options (pros / cons) |
|---|---|
| Jocasta in herdr | A plugin after a spike (its own panes, popup, keys, startup hook; preview on Windows) · Socket only (no preview feature; the question risk stays) |
| Questions | A popup in front (a thread can't type into it, only close it) · A line in `today` as 0552 says |
| `today` | Boss 65 / `today` 35 in the boss's workspace · Its own workspace · A popup on a key |

## Recommendation
A plugin after a spike; a popup; boss 65 / `today` 35.

## Choice
- `jocasta setup` links a `jocasta` herdr plugin shipped in the repo. It declares the `today` pane (0551), placed as a split beside the boss in the boss's workspace, boss 65 / `today` 35; an `ask` popup; actions for Finish, Stop, Review and New thread (0554's keys), running `jocasta thread …` (0544); and a startup hook that re-applies the agent view.
- Allow and close questions open in the `ask` popup in front of whatever he's on, with a one-time token passed through `--env`; Enter takes the default and Esc the fallback (0519). A popup closed by anything but his answer counts as Deny.
- `today` is plugin-owned: herdr detects no agent in it and Jocasta's thread list comes from omp (0547), so it's never a thread; 0518's heal and 0549's start reopen it with `plugin.pane.open`; 0521's restart stays for omp panes; H3 checks it apart (open, drawing, reopened after a kill).
- A spike on this PC checks link, the split pane, the popup (that `pane.send_text` can't reach it), the keys and the startup hook before the build. If plugins fail on Windows, the service opens `today` through `layout.apply` and questions stay in `today` as 0552 says.

## Reason
"Yes, after a spike"; "A popup in front"; "Boss 65 / today 35".

## Consequences
- Amends 0552: questions move from the `jocasta` pane into a popup. [Disproved by 0558: herdr's socket keys reach an open popup.] The belief at the time: herdr's socket could no longer answer one (only close it, which denies), leaving keystrokes synthesized into the terminal (Windows `SendInput` or UI Automation) as the residue accepted beside 0485.
- A sixth spike joins 0536's and 0542's four and 0552's audio helper.
- The plugin directory and `herdr plugin link` claim a path and a setup step in plan appendix I.
- Amended by 0558: the spike showed herdr's socket keys (`pane send-keys`) do reach an open popup; Allow therefore takes a typed 3-letter code. The plugin, split pane, popup, key bindings and startup hook work on Windows.
