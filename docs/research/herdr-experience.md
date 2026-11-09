# The herdr experience: a full setup proposal

Status: decided as records 0553–0555 (2026-11-09). Where his answers differ from this proposal, the records hold: the thread keys are Ctrl+B then Alt+F/S/R/N, and herdr's colours and default shell stay unchanged.

Sources, read on 2026-11-09 against the installed herdr 0.9.5: `herdr --default-config`, `herdr api schema --json` (128 methods and events), `herdr --help` and its subcommands, the 0.9.5 docs (plugins, socket API, Windows support), `config/herdr.toml` and his live `~/.jarvis/agent/herdr.toml`.

## 1. What herdr 0.9.5 offers that Jocasta doesn't use yet

| Feature | What it does | Windows |
|---|---|---|
| Plugins (`herdr-plugin.toml`) | A local package herdr links: actions (bindable to keys), event hooks, startup hooks, panes, link handlers. Commands are argv (Bun works), with `HERDR_BIN_PATH`, `HERDR_PLUGIN_STATE_DIR` and context JSON injected. | preview: "best-effort" |
| Plugin panes | `split`, `tab`, `overlay`, `zoomed` or `popup`. A split pane is a normal herdr pane owned by the plugin. | preview |
| Popups | A session-modal terminal over the layout; it "receives all terminal input, including Escape", closes when its command exits or on `popup.close`, and "has no pane ID … does not participate in pane, layout, persistence, or agent APIs". `plugin pane open --env` passes per-launch variables. | preview |
| `[[keys.command]]` | Binds a key to a shell command, a temporary pane, a popup, or a plugin action. | yes |
| `agent.view.set` | A declarative filter and sort for the Agents sidebar and next/previous agent navigation, by status, workspace or a reported metadata token. | yes |
| `ui.toast.delivery = "system"` | `notification.show` asks Windows' notification service directly, through the foreground client. | supported |
| `experimental.pane_history` | Keeps each pane's recent screen across a full server restart. | supported |
| `ui.status_indicators = "symbols"` | Distinct glyphs for blocked, working, done and idle instead of coloured dots. | yes |
| `ui.window_title` | The terminal window's title, from `{workspace}`, `{tab}`, `{pane}`. | yes |
| `theme.custom` | Per-token colour overrides on a built-in theme. | yes |

What Jocasta already uses: workspaces per thread, `layout.apply` to start omp, `pane.report_metadata` for `$model`, `$cost` and state labels, `workspace.report_metadata` for `$running`, the priority sort, Alt digits, the HUD command in the tab bar, toasts off.

## 2. The proposal

### 2.1 Jocasta is a herdr plugin
`jocasta setup` links a `jocasta` plugin (`herdr plugin link`), shipped in `config/herdr-plugin/`. It declares:
- **pane `today`**: the OpenTUI pane of 0551, placed as a split in the boss's workspace. herdr marks it plugin-owned, so it never counts as an agent or a thread; the service reopens it with `plugin.pane.open` if it exits (as 0521 does for omp).
- **popup `ask`**: the Allow and close questions (0552). The service opens it with a one-time token in `--env`; the popup's Bun process shows the question and sends the answer with that token. Disproved by the spike (0558): keys sent through herdr's socket (`pane send-keys` to any pane) reach an open popup, so the popup alone doesn't keep a thread from answering; 0558 adds a typed 3-letter code.
- **actions** bound to keys: Finish, Stop, Review (hunk beside the thread, 0406), New thread (a small popup asking project and brief), all running `jocasta thread …` (0544).
- **startup hook**: re-applies the agent view (2.4) after herdr restarts.

Plugins are preview on Windows; a spike checks link, split pane, popup, keys and the startup hook on this PC before the build.

### 2.2 Screen layout
```
┌ sidebar ───────────┬ Jocasta (Alt+1) ──────────────────────────┬ today ───────────────────┐
│ ▲ api/fix-login    │ boss (omp)                                │ Mon 9 Nov 14:05 ☁ 12°   │
│   needs: merge?    │                                           │ cpu 18% · mem 41%        │
│ ● web/docs  $0.84  │                                           │ quota 42% · $3.10 today  │
│ ✓ api/rate-limit   │                                           │ NEEDS YOU                │
│                    │                                           │ UNDERWAY                 │
│                    │                                           │ LANDED TODAY · NEXT      │
│                    │                                           │ NEWS (when rows remain)  │
└────────────────────┴───────────────────────────────────────────┴──────────────────────────┘
```
The boss's workspace splits 65/35: the boss left, `today` right. Projects keep their workspaces and each thread its linked-worktree workspace (Alt+2…9). The four extras fit: clock, weather and machine in the header, news at the bottom only when rows remain.

### 2.3 Questions and notifications
- Allow and close questions: the `ask` popup in front of whatever he's on, Enter for the default, Esc for the fallback (0519's defaults).
- Pings when the terminal isn't in front: `ui.toast.delivery = "system"` and the service calls `notification.show`; Windows shows it. herdr reports `no_foreground_client` when no terminal is attached, which only happens once Jocasta is closing (0549).
- Sound: `request` sound when a thread needs him, finished threads silent (proposal; decided in 0554).

### 2.4 Sidebar
- 0433 keeps herdr's sidebar; `status_indicators = "symbols"` for glyphs that read without colour.
- `agent.view.set` from the plugin: blocked and done first, then working, then idle, and the boss excluded (it has its own workspace).

### 2.5 Keys
| Key | Action | Today |
|---|---|---|
| Alt+1…9 | workspaces (boss = 1) | kept |
| Alt+G | goto | kept |
| Alt+[ / Alt+] | previous / next agent needing him | kept |
| Alt+Shift+F | Finish the thread in front | new |
| Alt+Shift+S | Stop the thread in front | new |
| Alt+Shift+R | Review (hunk beside) | new |
| Alt+Shift+N | New thread popup | new |
| Right Ctrl (hold) | dictate (0552) | kept |

### 2.6 Smaller settings
| Setting | Proposal | Why |
|---|---|---|
| `experimental.pane_history` | true | A healed server (0518) shows the panes' last screen instead of blank ones. |
| `ui.window_title` | `"Jocasta · {workspace}"` | The taskbar says which thread is in front. |
| `theme.custom` | `accent` and `sidebar_bg` from `palette.ts` | One palette for herdr and the pane. |
| `tab_bar_right` HUD | quota and spend move into `today`; the HUD keeps only what's needed when the boss's workspace isn't in front (blocked count and spend) | Not twice in front of him. |
| `terminal.default_shell` | `pwsh` when present (0505) | Shell panes match what threads run. |
| `ui.toast.delivery` | `"system"` | 2.3. |
| `update.version_check`, `manifest_check` | false (kept) | 0383, 0458. |

### 2.7 What a pane that isn't omp means
- It is plugin-owned; herdr's agent detection finds no agent in it, so it is absent from `agent.list` and the Agents sidebar.
- Jocasta's thread list comes from omp (0547), so it never counts the pane.
- 0518's heal and 0549's start reopen it with `plugin.pane.open`; 0521's restart applies to omp panes only.
- H3's checks count omp panes; `today` is checked separately (open, drawing, reopened after a kill).
