# UX proposal: Jocasta on herdr + companion (U, record 0395)

- Date: 2026-10-13
- Scope: the terminal work surface (herdr 0.9.4 session `jocasta`, record 0392), hunk review (0394) and the Electrobun companion (0392, 0393).
- How to use: one question per item, in order; each answer becomes a record. Unanswered items take the recommendation. The decided look goes into `DESIGN.md`; the decided herdr settings into `config/herdr.toml`.
- Paths: `herdr-src` = herdr v0.9.4 source (`Cargo.toml:3` version 0.9.4; docs under `docs/next/website/src/content/docs/`), `omp-src` = omp 18.10.3, `hunk-src` = hunk 0.23.0, `whiteboard-src` = whiteboard v0.2.4. Mocks are ASCII; colours are named in words.

## What herdr can express without a fork

Everything below is config or socket calls; none of the recommendations needs a herdr patch (0383, 0392).

| Surface | Knob | Evidence |
|---|---|---|
| Colours | `[theme] name`, `[theme.custom]` 19 tokens | `herdr-src/src/config/theme.rs:4-23`, `:102-126` |
| Sidebar rows | `[ui.sidebar.agents]`/`[ui.sidebar.spaces]` `rows`, styles, rules, `rows_by_agent` | `src/config/sidebar.rs:108-139`; `configuration.mdx:362-480` |
| Row data | `pane.report_metadata` / `workspace.report_metadata` tokens, `title`, `state_labels` | `socket-api.mdx:727-771`; `src/app/api_helpers.rs:205-208` |
| Tab-bar status | `ui.tab_bar_right` = zoom, hostname, datetime, text, command | `src/config/tab_bar.rs:23-41`; `configuration.mdx:318-336` |
| Notifications | `ui.toast.delivery` off/herdr/terminal/system; `notification.show` | `src/config/model.rs:63-69`; `socket-api.mdx:311-331` |
| Keys | `[keys]` incl. indexed `1..9`, `[[keys.command]]` popup/pane/shell | `src/config/model.rs:1120-1187`; `configuration.mdx:124-247` |
| Layout | `pane.split` direction + ratio, `layout.apply`, zoom | `socket-api.mdx:145-251` |

## 1. herdr palette

**Question.** Which palette does the terminal side (herdr, panes, omp) use?

**Facts.**
- herdr themes are 19 UI tokens (accent, panel_bg, sidebar_bg, active_row_bg, selection_bg, surface0/1, surface_dim, overlay0/1, text, subtext0, mauve, green, yellow, red, blue, teal, peach), not the 16 ANSI colours: `herdr-src/src/config/theme.rs:102-126`; 18 built-ins at `theme.rs:4-23`; the default is Catppuccin Mocha, `src/app/state.rs:77-99`.
- Pane content is not recoloured: ANSI palette indices pass through as indexed colours unless the program itself redefined them (`src/pane/terminal.rs:3597-3598`), so the outer terminal's scheme (Windows Terminal) paints ANSI 0-15 in every pane.
- herdr's state colours: working = `yellow`, blocked = `red`, done = `teal`, idle = `green`, unknown = `overlay0` (`src/client/shell.rs:225-236`). Its `terminal` theme maps those tokens to named ANSI colours (`src/app/state.rs:128-150`).
- omp draws its TUI from its own theme JSON (hex or 256-index), custom themes in `~/.omp/agent/themes/<name>.json` (omp `docs/theme.md:18-37`, `:254`). hunk's `theme = "terminal"` follows the ANSI palette (hunk `README.md:182`).
- Jocasta already maps the JARVIS look onto 16 ANSI colours for its xterm tabs: `src/frontier/ui/terminal.tsx:37-59` (record 0369 "in the look's ANSI palette").
- A colour scheme can ship as a Windows Terminal JSON fragment in `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\<app>\*.json`; a fragment scheme must define all 16 colours (https://learn.microsoft.com/windows/terminal/json-fragment-extensions).
- His words: "the colors and that stuff we do from 0 or maybe do the ui like herdr we still can decide" (0392); 0369: one look, cyan as light, champagne = awaiting you, rose = failed, sage = landed.

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| herdr default | Catppuccin Mocha in herdr; panes keep his current Windows Terminal scheme; omp keeps `titanium`. | Nothing to write or keep in sync; matches herdr's docs and screenshots. | Two looks: companion in JARVIS, terminal in Catppuccin; red = blocked and yellow = working break 0369's One Meaning rule. | `src/app/state.rs:77-99`; `src/client/shell.rs:225-236`; omp `docs/settings.md:856` |
| JARVIS on 16 ANSI (recommended) | One look everywhere: herdr `[theme.custom]` from DESIGN.md, a Windows Terminal scheme "Jocasta" (the 16 colours below) as a fragment, and an omp theme `jocasta.json`. | Terminal and companion read as one product; state colours keep their meaning (blocked = champagne, done = sage, working = arc); the mapping already exists in `terminal.tsx`. | Three files to keep in step (`config/herdr.toml`, the WT fragment, `config/omp-theme-jocasta.json`); `jocasta setup` gains a write outside `~/.jocasta` (the WT fragment, after his `y`); herdr's `red` also colours git "behind", which becomes champagne (DESIGN.md already uses champagne for "behind its remote"). | `src/config/theme.rs:102-126`; `src/pane/terminal.rs:3597-3598`; Jocasta `src/frontier/ui/terminal.tsx:37-59`; WT fragments (MS Learn) |
| From scratch | A new terminal-first palette designed in its own pass (e.g. graphite neutrals + one accent), applied through the same three files; the companion follows it. | Built for a terminal-first product instead of carrying the web look over. | Another design round before F2; reopens 0369 for the companion and canvas too; no existing mapping to reuse. | 0392 (his words); 0369 |
| herdr `terminal` + WT scheme | herdr's built-in `terminal` theme follows the ANSI palette; only the Windows Terminal "Jocasta" scheme is written. | One source of truth (the WT scheme); no herdr colour overrides. | `terminal` maps working -> Yellow (= champagne), blocked -> LightRed, done -> Cyan (`state.rs:143-147`): champagne would mean "working", against 0369; active row is plain DarkGray. | `src/app/state.rs:128-150` |

**16-colour table for the JARVIS option** (Windows Terminal scheme "Jocasta"; same values as `src/frontier/ui/terminal.tsx:37-59`; background ink-0 `#06090f`, foreground code-ink `#c9d5e2`, cursor arc `#92d6ec`, selection `#1d3242`):

| # | ANSI | DESIGN.md token | Hex |
|---|---|---|---|
| 0 | black | ink-2 | `#0e1520` |
| 1 | red | rose | `#e39a9a` |
| 2 | green | sage | `#9cc9b0` |
| 3 | yellow | champagne (gold) | `#d9c08f` |
| 4 | blue | code-blue | `#8fb4e8` |
| 5 | magenta | code-violet | `#c4a7e7` |
| 6 | cyan | arc | `#92d6ec` |
| 7 | white | code-ink | `#c9d5e2` |
| 8 | brightBlack | text-3 | `#6f839b` |
| 9 | brightRed | rose, lighter | `#f0b4b4` |
| 10 | brightGreen | sage, lighter | `#b8dcc8` |
| 11 | brightYellow | champagne, lighter | `#e8d4ad` |
| 12 | brightBlue | code-blue, lighter | `#b0cbf0` |
| 13 | brightMagenta | code-violet, lighter | `#d8c3f0` |
| 14 | brightCyan | arc-hi | `#c4ecf8` |
| 15 | brightWhite | text, lighter | `#eef4fa` |

**herdr `[theme.custom]` for the JARVIS option:**

| herdr token (role) | Value |
|---|---|
| accent | arc `#92d6ec` |
| panel_bg | ink-0 `#06090f` |
| sidebar_bg | ink-1 `#0a0f17` |
| active_row_bg | ink-2 `#0e1520` |
| selection_bg | ink-3 `#131c29` |
| surface0 / surface1 / surface_dim | ink-2 `#0e1520` / ink-3 `#131c29` / ink-1 `#0a0f17` |
| overlay0 / overlay1 | text-3 `#6f839b` / text-2 `#93a3b7` |
| text / subtext0 | text `#dde7f1` / text-2 `#93a3b7` |
| mauve (branch) | code-violet `#c4a7e7` |
| yellow (working) | arc `#92d6ec` |
| red (blocked) | champagne `#d9c08f` |
| teal (done) | sage `#9cc9b0` |
| green (idle) | text-3 `#6f839b` |
| blue (unseen) | arc `#92d6ec` |
| peach (interrupted) | rose `#e39a9a` |

**Mocks.**

*herdr default*

```text
herdr default: Catppuccin Mocha (UI only; panes keep the WT scheme)
 Spaces                  │ 1 add retry  2 export v2
 ● Jocasta               │──────────────────────────────────────────
 ● acme-api      [red]   │ omp  acme-api · export v2
   main · ↑2 ↓1          │  ? Use the v1 or the v2 schema?
 ● client-a-web  [yellow]│    > v2   v1
 Agents                  │
 ● acme-api · export v2  │
   omp                   │
 ● client-a-web · retry  │
   omp                   │

 tokens   accent #89b4fa  panel #181825  text #cdd6f4
 states   working yellow #f9e2af   blocked red #f38ba8
          done teal #94e2d5        idle green #a6e3a1
```

*JARVIS on 16 ANSI (recommended)*

```text
JARVIS on 16 ANSI  (WT scheme "Jocasta" + herdr [theme.custom])
  #  ansi           token        hex       herdr token -> hex
  0  black          ink-2        #0e1520   panel_bg      ink-0 #06090f
  1  red            rose         #e39a9a   sidebar_bg    ink-1 #0a0f17
  2  green          sage         #9cc9b0   active_row_bg ink-2 #0e1520
  3  yellow         champagne    #d9c08f   selection_bg  ink-3 #131c29
  4  blue           code-blue    #8fb4e8   accent        arc   #92d6ec
  5  magenta        code-violet  #c4a7e7   text          text  #dde7f1
  6  cyan           arc          #92d6ec   overlay0/1    #6f839b/#93a3b7
  7  white          code-ink     #c9d5e2   yellow=working arc  #92d6ec
  8  brightBlack    text-3       #6f839b   red=blocked   champagne #d9c08f
  9  brightRed                   #f0b4b4   teal=done     sage  #9cc9b0
 10  brightGreen                 #b8dcc8   green=idle    text-3 #6f839b
 11  brightYellow                #e8d4ad   peach=warn    rose  #e39a9a
 12  brightBlue                  #b0cbf0   blue=unseen   arc   #92d6ec
 13  brightMagenta               #d8c3f0   mauve=branch  #c4a7e7
 14  brightCyan     arc-hi       #c4ecf8
 15  brightWhite                 #eef4fa   bg #06090f fg #c9d5e2
                                           cursor #92d6ec sel #1d3242
 Spaces                  │ 1 add retry  2 export v2
 ● Jocasta               │──────────────────────────────────────
 ● acme-api   champagne  │ omp  ? Use the v1 or the v2 schema?
 ● client-a-web  arc     │
```

*From scratch*

```text
From scratch: a separate palette pass (illustrative only)
 Spaces                  │ 1 add retry  2 export v2
 ● Jocasta               │──────────────────────────────────────
 ● acme-api   <blocked>  │ omp  ? Use the v1 or the v2 schema?
 ● client-a-web <working>│
 Agents                  │
 ● acme-api · export v2  │
   needs you             │

 1. pick neutrals (bg / raised / rule / text x3)
 2. pick one accent + three state hues
 3. derive 16 ANSI + herdr 19 tokens + omp theme
 4. re-skin companion + canvas to match
 -> one more walkthrough before F2
```

*herdr `terminal` + WT scheme*

```text
herdr name = "terminal" + WT scheme "Jocasta"
 Spaces                  │ 1 add retry  2 export v2
 ● Jocasta               │──────────────────────────────────────
 ● acme-api   bright rose│ omp  ? Use the v1 or the v2 schema?
 ● client-a-web champagne│
                         │
 working -> ANSI yellow  = champagne #d9c08f  (means "awaiting you"!)
 blocked -> ANSI lightred= #f0b4b4
 done    -> ANSI cyan    = arc #92d6ec
 idle    -> ANSI green   = sage #9cc9b0
 active row -> DarkGray (brightBlack #6f839b)
```

**Recommendation: JARVIS on 16 ANSI.** It is the only option that keeps one look across the two surfaces and keeps 0369's colour meanings in herdr's state marks; the ANSI mapping already exists, so the cost is three small config files.

## 2. Sidebar rows

**Question.** What does each sidebar row show for a project (Space) and a thread (Agent)?

**Facts.**
- Agent-row tokens: `state_icon`, `state_text`, `machine`, `workspace`, `tab`, `pane`, `agent`, `terminal_title`, `terminal_title_stripped`, `$name` (`herdr-src/src/config/sidebar.rs:108-125`); Space-row tokens: `state_icon`, `state_text`, `workspace`, `branch`, `git_status`, `$name` (`sidebar.rs:127-139`); at most 16 rows x 16 tokens (`sidebar.rs:11-12`). Defaults: `docs/.../configuration.mdx:366-380`.
- `$name` values come from `pane.report_metadata` / `workspace.report_metadata`: up to 16 keys per report, 32 per pane, key <= 32 chars, value <= 80 chars (`src/app/api_helpers.rs:205-208`); the same call can set `title`, `display_agent` and `state_labels` per state (`docs/.../socket-api.mdx:727-755`). Tokens are display-only and not restored after a server restart (`socket-api.mdx:769`).
- Tokens can be styled with `fg`/`bold`/`dim` and up to 16 rules (`equals`, `contains`, `starts_with`, `gt`, `lt`, `hide`) (`configuration.mdx:406-435`); `rows_by_agent` overrides rows for one agent id; the OMP integration reports the label `omp` (`integrations.mdx:102`).
- `ui.agent_panel_sort = "priority"` orders agents by state (`config-reference.json:1044-1051`): blocked 4 > done 3 > working 2 (`src/client/shell.rs:203-208`). Sidebar width default 26 columns, 18-36 (`src/config/model.rs:940-944`, `:1201`).
- No Jocasta code runs inside omp (0386), so `$model`, `$cost` and labels are reported by the companion's service for the panes `thread_start` created, read from each pane's newest omp session file (plan F2 step 3).

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| herdr default rows | Agents: `[state_icon, machine, workspace, tab]`, `[agent]`; Spaces: `[state_icon, workspace]`, `[branch, git_status]`. | No reporting code; stable. | Second agent row always says `omp`; no model or cost (PRODUCT principle 1: "cost readable in under a second"). | `configuration.mdx:366-380` |
| Two lines: state, model, cost (recommended) | Agents: `[state_icon, workspace, tab]`, `[state_text, $model, $cost]`; Spaces: `[state_icon, workspace, $running]`, `[branch, git_status]`; `state_labels` blocked = the question, done = "review ready"; priority sort. | Project, thread, state, model and cost at a glance; blocked first; the question is visible without opening the tab. | Needs the service to report tokens (lost on herdr restart until the next report); row 2 truncates at ~24 columns. | `sidebar.rs:108-139`; `api_helpers.rs:205-208`; `shell.rs:203-208` |
| One dense line | Agents: `[state_icon, workspace, tab, $cost]`; Spaces: `[state_icon, workspace, git_status]`. | Twice as many threads fit; minimal. | No model, no state words; the question hides behind the icon. | `sidebar.rs:108-139` |
| Three lines with summary | Agents: `[state_icon, tab, state_text]`, `[$summary]` (last assistant line), `[workspace, $model, $cost]`. | Reads like a digest: what each thread is doing now. | Three rows per thread; the summary changes often (churn against "quiet when idle"); more reports. | `configuration.mdx:458-476` |

**Mocks.**

*herdr default rows*

```text
herdr default rows (sidebar 26 cols)
 Spaces
 ● Jocasta
 ● acme-api
   main · ↑2
 ● client-a-web
   main
 Agents
 ● acme-api · export v2
   omp
 ● client-a-web · add retry
   omp
 ○ Jocasta · boss
   omp
```

*Two lines: state, model, cost (recommended)*

```text
Two lines: state, model, cost  (priority sort)
 Spaces
 ● Jocasta
 ● acme-api · 1 at work
   main · ↑2
 ● client-a-web · 1 at work
   main
 Agents
 ● acme-api · export v2
   needs you: v1 or v2? · opus · $0.84
 ● client-a-web · add retry
   review ready · sonnet · $1.10
 ● acme-api · flaky checkout
   working · opus · $0.31
 ○ Jocasta · boss
   idle · opus · $0.12

 [ui.sidebar.agents]
 rows = [["state_icon","workspace","tab"],
         ["state_text","$model","$cost"]]
```

*One dense line*

```text
One dense line
 Spaces
 ● Jocasta
 ● acme-api ↑2
 ● client-a-web
 Agents
 ● acme-api · export v2 · $0.84
 ● client-a-web · add retry · $1.10
 ● acme-api · flaky checkout · $0.31
 ○ Jocasta · boss · $0.12
```

*Three lines with summary*

```text
Three lines with summary
 Agents
 ● export v2 · needs you
   Use the v1 or the v2 schema?
   acme-api · opus · $0.84
 ● add retry · review ready
   Added retry with backoff; 2 commits.
   client-a-web · sonnet · $1.10
 ● flaky checkout · working
   Running the checkout tests…
   acme-api · opus · $0.31
```

**Recommendation: Two lines: state, model, cost.** It covers every field asked for (project, thread, state, model, cost) in two rows, puts blocked threads first, and shows the blocking question in the row itself; the reporting reuses the session-file read `thread_status` already needs.

## 3. HUD in the tab bar

**Question.** How does the HUD (at work, awaiting you, today's spend) show in herdr?

**Facts.**
- `ui.tab_bar_right` entries: `zoom`, `hostname`, `datetime`, `text`, `command` (`herdr-src/src/config/tab_bar.rs:23-41`), at most 16 (`tab_bar.rs:7`).
- A `command` entry runs at `interval_seconds` (1 s-1 y), on Windows through `cmd.exe /d /c`; herdr keeps the last output line, strips escape sequences (no colour), clears it on failure or timeout, and yields the area to tabs on a narrow row (`docs/.../configuration.mdx:318-336`; `src/app/tab_bar_status.rs:91-110`). Separators draw in `overlay0` (`src/client/shell/tabs.rs:275-276`).
- Workspace tokens render in Space rows and accept `gt`/`lt` rules, so a `$hud` on the Jocasta row can turn champagne when `awaiting > 0` (`configuration.mdx:418-435`).
- The outer window title takes `{workspace}`, `{tab}`, `{pane}` tokens (`configuration.mdx:340-351`).
- 0370's HUD text: "3 at work  1 awaiting you  $4.82 today  62% left".

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| Tab-bar line from a file (recommended) | The service writes `<data>/hud.txt` on every change; `tab_bar_right = [{ type = "command", command = "type <data>\\hud.txt", interval_seconds = 2 }, { type = "datetime" }]`. | Always on screen; no new Jocasta command; `type` is a cmd built-in, so each tick is cheap. | Plain text, no champagne; up to 2 s late; shows a stale line if the companion dies (it clears the file on quit). | `tab_bar.rs:23-41`; `configuration.mdx:318-336` |
| `jocasta hud` command | Same line, printed by a new `jocasta hud` subcommand that asks the service. | Always fresh; no file. | A Bun process start every interval; adds a command outside 0387's set (`jocasta`, `setup`, `mcp`, `project`). | `configuration.mdx:334`; record 0387 |
| Jocasta sidebar row | No tab-bar entry; the service reports workspace tokens `$at_work`, `$awaiting`, `$spend` on the Jocasta Space row, styled by rules. | Coloured: `awaiting > 0` turns champagne; no process spawns. | Hidden when the sidebar is collapsed; sits in the sidebar, not the tab bar. | `configuration.mdx:406-435`; `socket-api.mdx:757-763` |
| No HUD in herdr | Counts live in herdr's own marks; spend and usage show only in the companion. | Nothing to build. | Spend is not visible where he works. | PRODUCT.md principle 1 |

**Mocks.**

*Tab-bar line from a file (recommended)*

```text
Tab-bar line from a file
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1 export v2  2 flaky checkout   3 at work · 1 awaiting you · $4.82 today · 14:32│
├──────────────────────────────────────────────────────────────────────────────┤
│ omp  acme-api · export v2                                                    │

 config/herdr.toml
 [ui]
 tab_bar_right = [
   { type = "command", command = "type <data>\hud.txt", interval_seconds = 2 },
   { type = "datetime", format = "%H:%M" },
 ]
 tab_bar_right_separator = " · "
```

*`jocasta hud` command*

```text
`jocasta hud` command
│ 1 export v2  2 flaky checkout   3 at work · 1 awaiting you · $4.82 today · 14:32│

 tab_bar_right = [
   { type = "command", command = "jocasta hud", interval_seconds = 5 },
 ]
 cost: one jocasta.exe start per 5 s (~tens of ms CPU each)
```

*Jocasta sidebar row*

```text
Jocasta sidebar row
 Spaces
 ● Jocasta
   3 at work · 1 awaiting you · $4.82   <- "awaiting" in champagne
 ● acme-api
   main · ↑2

 [ui.sidebar.spaces]
 rows = [["state_icon","workspace"],
   ["$at_work", {token="$awaiting", rules=[{gt=0, fg="#d9c08f"}]}, "$spend"],
   ["branch","git_status"]]
```

*No HUD in herdr*

```text
No HUD in herdr
│ 1 export v2  2 flaky checkout                                          14:32│
├──────────────────────────────────────────────────────────────────────────────┤
 (spend and "awaiting you" only in the companion window)
```

**Recommendation: Tab-bar line from a file.** It puts state and spend on screen at all times without a new command or patch; colour is carried by the sidebar's state marks, so plain text in the tab bar is enough.

## 4. The boss pane

**Question.** Where does the boss pane sit, how big is it, and what does it show?

**Facts.**
- 0392: workspace "Jocasta" holds the boss pane; the boss is omp with the boss manual appended and `jocasta mcp --boss`.
- A `popup` custom command closes when its command exits and gets no `HERDR_PANE_ID` (`docs/.../configuration.mdx:204-210`), so a popup cannot be the boss's permanent home; a `shell` custom command can call `herdr` with `HERDR_*` env (`configuration.mdx:228-247`), e.g. to focus the boss (`agent.focus`, `socket-api.mdx:109`).
- omp's status line can be `custom` with left/right segments (model, path, git, context, spend, cost, subagents) (omp `docs/settings.md:839-876`, `docs/theme.md:70-72`); `display.pinnedAgents` lists live subagents above the editor (`settings.md:872`).
- PRODUCT principle 4: the harness stands alone; Jocasta adds orchestration and never hides the TUI.

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| Own workspace, full tab (recommended) | Workspace "Jocasta" is first (Alt+1, item 8); one tab, one pane with omp; status line: model · context left, spend · subagents right. | Same mechanics as every thread; full width for the boss; events land in one place. | Boss and a thread are never visible together (a key apart). | record 0392; omp `docs/settings.md:839-876` |
| Boss + scratch shell | Workspace "Jocasta": boss 65% left, a PowerShell pane in `<data>/home` 35% right for quick `git`/`jocasta project` commands. | Commands without leaving the boss. | Boss loses a third of its width; the shell is rarely needed (threads have their own). | `socket-api.mdx:152` (split ratio) |
| Boss as a popup | No permanent pane: a key opens a popup running `omp --continue` in `<data>/home` over any tab. | Summoned over whatever he is looking at. | The popup ends with its command, so events can't reach a closed boss; a second omp on the same session if a pane also exists; contradicts 0392's boss pane. | `configuration.mdx:204-210` |

**Mocks.**

*Own workspace, full tab (recommended)*

```text
Own workspace, full tab
 Spaces             │ 1 boss                     3 at work · 1 awaiting you · $4.82
 ● Jocasta          │────────────────────────────────────────────────────────────
 ● acme-api         │ you › how is acme-api doing?
 ● client-a-web     │ omp › Two threads. Flaky checkout is running tests;
                    │       export v2 is waiting on you: v1 or v2 schema?
 Agents             │ [event] client-a-web/add retry: review ready, 3 notes
 ● acme-api · exp…  │
 ● client-a… · re…  │ > _
 ○ Jocasta · boss   │ opus · ctx 18% ··················· $0.12 · 0 agents
```

*Boss + scratch shell*

```text
Boss + scratch shell
 Spaces       │ 1 boss
 ● Jocasta    │ you › how is acme-api doing?          │ PS> jocasta project list
 ● acme-api   │ omp › Two threads. Flaky checkout is  │ acme-api      D:\src\acme-api
 ● client-a…  │       running tests; export v2 waits  │ client-a-web  D:\src\client-a
              │       on you: v1 or v2 schema?        │ PS> _
              │ > _                                   │
```

*Boss as a popup*

```text
Boss as a popup (prefix+space)
 Spaces       │ 1 export v2  2 flaky checkout
 ● Jocasta    │ omp  acme-api · export v2
 ● acme-api   │   ┌──────────── boss ─────────────────┐
 ● client-a…  │   │ you › tell export to use v2       │
              │   │ omp › Sent.                       │
              │   │ > _                               │
              │   └───────────────────────────────────┘
```

**Recommendation: Own workspace, full tab.** It matches 0392 and keeps the boss a normal omp pane that can receive events while he works elsewhere; Alt+1 (item 8) brings it back from anywhere.

## 5. Blocked threads and the boss's events

**Question.** How do blocked threads and Jocasta's events reach him?

**Facts.**
- herdr's OMP extension reports `blocked` on a tool-approval request and while the `ask` tool runs, with the question as the message, and `working`/`idle` otherwise (`herdr-src/src/integration/assets/omp/herdr-agent-state.ts:281-292`, `:404-437`).
- A blocked agent makes its pane, tab and workspace look blocked; `done` means finished and not yet viewed (`docs/.../agents.mdx:123`, `concepts.mdx:46-51`).
- `state_labels` can relabel each state per pane (`socket-api.mdx:739-753`); `agent.prompt` refuses a blocked target with `agent_blocked` (`socket-api.mdx:114`).
- Plan F2 step 6: events go to the boss as `prompt(bossPane, "[event] …")`, queued until the boss pane is idle unless urgent (events and interjections 0298-0303).

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| herdr only | Blocked shows as herdr state + notification; the boss hears nothing unless asked (`thread_status`). | No injection; boss context stays clean. | The boss can't relay or batch; "review ready" (0394) has no home. | `herdr-agent-state.ts:404-437` |
| herdr + boss events (recommended) | herdr shows blocked at once; the service also queues one `[event]` line per change (blocked, done, failed, review ready) into the boss pane when it is idle; urgent events go in at once. | One place to read what happened; the boss can answer through `thread_send`; matches the plan. | Each event costs a boss turn (tokens); a busy boss sees them late. | plan F2 step 6; `socket-api.mdx:114` |
| Events + spoken | As the recommended option, plus the companion speaks one short line per blocked/review-ready event ("Export would like a word."). | He hears it away from the screen (V-film feel). | Noise while he is at the terminal; needs a rule for when to stay quiet. | record 0373 (speech per message); DESIGN.md:252 |
| Digest only | No per-event lines; the boss gets one digest (Needs you / Landed / Underway / Next, 0332) when he asks or every 30 min. | Fewest boss turns. | Late; blocked threads wait on herdr alone. | record 0332 (digest groups) |

**Mocks.**

*herdr only*

```text
herdr only
 Agents
 ● acme-api · export v2        <- champagne, first (priority)
   needs you: v1 or v2? · opus · $0.84
 ● client-a-web · add retry    <- sage (done, unseen)
   review ready · sonnet · $1.10
                                  ┌───────────────────────────────┐
                                  │ omp needs attention            │
                                  │ acme-api · export v2           │
                                  └───────────────────────────────┘
```

*herdr + boss events (recommended)*

```text
herdr + boss events
 Spaces             │ 1 boss
 ● Jocasta          │ [event] acme-api/export v2 needs you: v1 or v2 schema?
 ● acme-api         │ [event] client-a-web/add retry: review ready, 3 notes
 ● client-a-web     │ omp › Export v2 is asking which schema to use. Add
                    │       retry finished; hunk has 3 reviewer notes.
 Agents             │       Shall I tell export to use v2?
 ● acme-api · exp…  │ you › yes
   needs you: v1…   │ omp › Sent to export v2.
 ● client-a… · re…  │ > _
```

*Events + spoken*

```text
Events + spoken
 boss pane:  [event] acme-api/export v2 needs you: v1 or v2 schema?
 companion:  ◎ (breath)  "Export would like a word."
 boss pane:  [event] client-a-web/add retry: review ready, 3 notes
 companion:  ◎ (breath)  "Add retry is ready for review." 
```

*Digest only*

```text
Digest only
 omp › Digest 14:30
       Needs you   export v2 (v1 or v2 schema?)
       Landed      add retry (3 review notes)
       Underway    flaky checkout
       Next        —
```

**Recommendation: herdr + boss events.** herdr already draws blocked state and the question; adding queued `[event]` lines gives the boss what it needs to relay and act, which is the plan's design, without speaking over his work.

## 6. Review layout

**Question.** Where does hunk open when a thread finishes?

**Facts.**
- hunk's auto layout keeps split view at >= 120 columns, shows its file sidebar at >= 160 and the full layout at >= 220 (`hunk-src/packages/hunk/src/lib/responsive.ts:5-8`); `mode = auto | split | unified` (hunk `README.md:182-196`); agent notes dock beside the code at >= 84 columns in split view (`src/lib/agentNoteGeometry.ts:43-44`).
- `pane.split` takes `direction` `right`|`down` and `ratio` (`docs/.../socket-api.mdx:78`, `:152`); `prefix+z` zooms a pane (`keyboard.mdx:38`).
- Width estimate [INFERENCE]: Windows Terminal's default Cascadia Mono 12 pt (https://learn.microsoft.com/windows/terminal/customize-settings/profile-appearance) is about 9 px per cell, ~210 columns on a 1920 px screen, ~184 after herdr's 26-column sidebar.
- His words for 0394: "hunk beside the thread". Superset and Emdash put the diff in a right-hand panel beside the agent (https://docs.superset.sh/diff-viewer, https://emdash.com/docs/diff-view).

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| Beside, half width (recommended) | `splitBeside(pane, hunk…)` = `direction: right, ratio: 0.5`; hunk in auto mode (unified at ~92 columns); `prefix+z` zooms hunk to full width for split view. | Thread and diff side by side, as he said; zoom gives the wide view on demand. | At half width hunk is unified and hides its file list until zoomed. | `responsive.ts:5-8`; `socket-api.mdx:152` |
| Under the thread | `direction: down, ratio: 0.5`: hunk gets the full width (~184 columns: split view + file sidebar) and half the height. | hunk's best layout without zoom; thread stays visible above. | ~20 rows each; omp's transcript gets short. | `responsive.ts:5-8` |
| Own tab | `tab.create` "review · add retry" in the project workspace, hunk full screen. | Full split view and sidebar; thread untouched. | Thread and diff never visible together; one more tab per finished thread. | `socket-api.mdx:105` (tab.create) |
| Beside, hunk wider | `direction: right, ratio: 0.35`: hunk ~120 columns (split view), thread ~64 columns. | Split view beside the thread without zoom. | omp at ~64 columns wraps hard; depends on his font and screen. | `responsive.ts:6` |

**Mocks.**

*Beside, half width (recommended)*

```text
Beside, half width
 1 add retry ✓   2 flaky checkout
┌──────────────────────────────┬───────────────────────────────┐
│ omp  client-a-web · add retry│ hunk  main...add-retry  +42 -7│
│ omp › Done: retry with       │ src/http.ts                   │
│       backoff, 2 commits.    │  12   const res = await f…    │
│ [review] Running reviewer…   │ +13   for (let i = 0; i < 3…  │
│ omp › Posted 3 notes.        │ +14     await sleep(2 ** i…   │
│ > _                          │  ┃ note: no jitter; add ±20%  │
└──────────────────────────────┴───────────────────────────────┘
```

*Under the thread*

```text
Under the thread
┌──────────────────────────────────────────────────────────────┐
│ omp › Done: retry with backoff, 2 commits. Posted 3 notes.   │
│ > _                                                          │
├──────────────────────────────────────────────────────────────┤
│ hunk  main...add-retry                                       │
│ files      │ src/http.ts (old)        │ src/http.ts (new)    │
│ http.ts  3 │ 12 const res = await f…  │ 12 const res = awa…  │
│ http.test  │                          │ 13 for (let i = 0;…  │
│            │                          │ ┃ note: no jitter    │
└──────────────────────────────────────────────────────────────┘
```

*Own tab*

```text
Own tab
 1 add retry ✓  2 review · add retry   3 flaky checkout
┌──────────────────────────────────────────────────────────────┐
│ hunk  main...add-retry                         3 notes       │
│ files      │ src/http.ts (old)        │ src/http.ts (new)    │
│ http.ts  3 │ 12 const res = await f…  │ 12 const res = awa…  │
│ http.test  │                          │ 13 for (let i = 0;…  │
│            │                          │ ┃ note: no jitter    │
└──────────────────────────────────────────────────────────────┘
```

*Beside, hunk wider*

```text
Beside, hunk wider
┌───────────────────┬──────────────────────────────────────────┐
│ omp  add retry    │ hunk  main...add-retry                   │
│ omp › Done: retry │ src/http.ts (old)    │ src/http.ts (new) │
│  with backoff,    │ 12 const res = aw…   │ 12 const res = …  │
│  2 commits.       │                      │ 13 for (let i =…  │
│ > _               │                      │ ┃ note: no jitter │
└───────────────────┴──────────────────────────────────────────┘
```

**Recommendation: Beside, half width.** It is what he asked for in 0394, keeps both readable, and the wide split view is one `prefix+z` away; the split ratio is a single number to change later.

## 7. Notifications

**Question.** Who raises notifications: herdr, the companion, or both?

**Facts.**
- herdr `ui.toast.delivery`: `off` (default), `herdr` (in-app), `terminal` (outer terminal), `system` (OS) (`herdr-src/src/config/model.rs:63-69`); herdr suppresses popups for the active tab; `delay_seconds` debounces (`docs/.../configuration.mdx:484-495`).
- On Windows `system` toasts stay in Notification Center; clicking one selects the originating pane and brings the terminal forward (`configuration.mdx:499`). Sounds per kind: `done_path`, `request_path` (`configuration.mdx:501-520`).
- `notification.show` lets the service raise its own through the same delivery: title <= 80, body <= 240 chars; results `shown`, `disabled`, `rate_limited`, `no_foreground_client`, `busy` (`socket-api.mdx:311-331`).
- 0329: companion styled toasts when focused, OS notifications when not; 0332 texts: `<thread> needs you: …`, `<thread>: done — <summary>`, 6 s bottom right.

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| herdr in-app only | `delivery = "herdr"`; companion raises none. | One source; respects which tab he is viewing. | Invisible when the terminal is minimised or he is in another app. | `configuration.mdx:484-495` |
| herdr system + companion for its own (recommended) | herdr `delivery = "system"` for thread states (Windows toasts, click focuses the pane, sounds on); the companion toasts only what herdr doesn't know (reminders, pollers, voice errors), in-window when focused, OS otherwise (0329). | Seen from any app; click lands on the pane; no duplicates (each event has one owner). | Thread toasts are plain Windows toasts, not the JARVIS style. | `model.rs:63-69`; `configuration.mdx:499`; record 0329 |
| Companion for everything | herdr `off`; the service listens to `pane.agent_status_changed` and raises JARVIS-styled toasts for every event. | One styled look. | The companion doesn't know which tab he is viewing (duplicates for the active tab); click can only call `agent.focus`, not raise the terminal window reliably. | `socket-api.mdx:109`; record 0329 |
| Both for thread events | herdr in-app toasts and companion OS toasts for the same events. | Hard to miss. | Every event twice. | — |

**Mocks.**

*herdr in-app only*

```text
herdr in-app only
│ omp  acme-api · flaky checkout                                    │
│                                      ┌──────────────────────────┐ │
│                                      │ ● export v2 needs you    │ │
│                                      │   acme-api · v1 or v2?   │ │
│                                      └──────────────────────────┘ │
```

*herdr system + companion for its own (recommended)*

```text
herdr system + companion for its own
 Windows notification (from herdr)      ┌─────────────────────────────┐
                                        │ herdr                       │
                                        │ export v2 needs you         │
                                        │ acme-api · v1 or v2 schema? │
                                        └─────────────────────────────┘
                                          click -> herdr selects the pane
 Companion toast (its own events only)
   ◎ Reminder: send client-a the invoice · 15:00
```

*Companion for everything*

```text
Companion for everything
                       ┌────────────────────────────────────────┐
                       │ ◎ export v2 needs you: v1 or v2?        │
                       │ ◎ add retry: done — 2 commits, 3 notes  │
                       └────────────────────────────────────────┘
   (even when he is already looking at that tab)
```

*Both for thread events*

```text
Both
 herdr toast:     ● export v2 needs you
 Windows toast:   ◎ export v2 needs you: v1 or v2?
 (same event, two places)
```

**Recommendation: herdr system + companion for its own.** Each event gets exactly one owner: herdr knows what he has seen and can focus the pane on click; the companion keeps the styled toasts for the things only it knows.

## 8. Keys

**Question.** Which keys move him around herdr, and what changes from herdr's defaults?

**Facts.**
- herdr defaults: prefix `ctrl+b`; `prefix+1..9` tabs; `prefix+g` goto; `prefix+w` workspaces; `prefix+b` sidebar; `switch_workspace`, `focus_agent`, `next_agent` unset (`herdr-src/src/config/model.rs:1123-1182`). Direct chords are allowed; herdr's docs suggest `ctrl+alt+…` for prefix-free use (`docs/.../keyboard.mdx:106-140`).
- omp binds `Ctrl+B` (cursor left, `omp-src/packages/tui/src/keybindings.ts:62`), `Ctrl+Alt+]` (jump backward, `keybindings.ts:90`), and `Ctrl+P/O/T/G/Q/R/L`, `Alt+A/H/L/M/P/R`, `Space` (push-to-talk) (omp `docs/keybindings.md:29-56`). No omp default uses `Alt+digit`.
- [INFERENCE] On Windows, Ctrl+Alt is AltGr on layouts that have one, so `ctrl+alt+letter` chords can type characters instead; `Ctrl+digit` has no legacy terminal encoding, so 0370's Ctrl+1-9 may not reach herdr through Windows Terminal (check before adopting).
- Right Ctrl dictation is a global hook in the companion (0392), outside herdr.

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| herdr defaults | Learn `ctrl+b` + key; nothing changes. | Matches herdr docs; nothing to maintain. | Two keystrokes for every jump; `Ctrl+B` lost to omp's cursor-left inside panes (Left arrow still works). | `model.rs:1123-1182`; omp `keybindings.ts:62` |
| Alt digits for workspaces (recommended) | Keep the `ctrl+b` prefix; add `switch_workspace = ["prefix+shift+1..9", "alt+1..9"]` (Alt+1 = Jocasta/boss), `goto = ["prefix+g", "alt+g"]`, `next_agent`/`previous_agent` = `alt+]`/`alt+[`. | One chord to the boss and to each project; no clash with omp's defaults; prefix bindings stay for everything else. | Shells lose Alt+digit (PSReadLine digit-argument); workspace order decides the digit. | `model.rs:1156`; omp `docs/keybindings.md:29-56` |
| herdr's ctrl+alt set | herdr's documented prefix-free set: `ctrl+alt+h/j/k/l` panes, `ctrl+alt+[`/`]` tabs, `ctrl+alt+c` new tab, `ctrl+alt+z` zoom. | Vetted by herdr across terminals. | `ctrl+alt+]` is omp's jump-backward; AltGr layouts may type characters; no workspace jump. | `keyboard.mdx:114-128`; omp `keybindings.ts:90` |
| Ctrl+1-9 as in 0370 | `switch_tab = "ctrl+1..9"` like the old crew pills. | Same keys as the web Frontier. | May never reach herdr in Windows Terminal (no legacy encoding) [INFERENCE]; tabs, not projects. | record 0370; `keybinds.rs:979` |

**Mocks.**

*herdr defaults*

```text
herdr defaults
 ctrl+b 1..9    tab 1-9          ctrl+b g      goto picker
 ctrl+b w       workspaces       ctrl+b b      sidebar
 ctrl+b z       zoom             ctrl+b ?      help
 ctrl+b o       open last notification target
 boss: ctrl+b w, pick Jocasta
```

*Alt digits for workspaces (recommended)*

```text
Alt digits for workspaces
 alt+1          Jocasta (boss)   alt+2..9      project workspaces
 alt+g          goto picker      alt+] / alt+[ next / previous agent
 ctrl+b 1..9    tab in workspace ctrl+b z      zoom (hunk wide view)
 ctrl+b o       last notification target
 right ctrl     dictate to the boss (companion, global)

 [keys]
 switch_workspace = ["prefix+shift+1..9", "alt+1..9"]
 goto = ["prefix+g", "alt+g"]
 next_agent = "alt+]"
 previous_agent = "alt+["
```

*herdr's ctrl+alt set*

```text
herdr's ctrl+alt set
 ctrl+alt+h/j/k/l   focus pane      ctrl+alt+z   zoom
 ctrl+alt+[ / ]     prev/next tab   ctrl+alt+c   new tab
 conflict: ctrl+alt+]  = omp jump backward
 risk:     AltGr layouts (ctrl+alt+letter types a char)
```

*Ctrl+1-9 as in 0370*

```text
Ctrl+1-9 as in 0370
 ctrl+1..9   tab 1-9 in the current workspace
 needs a check: does Windows Terminal deliver ctrl+digit to herdr?
```

**Recommendation: Alt digits for workspaces.** Alt+digit is free in omp and in herdr's defaults, gives one-chord jumps to the boss and every project, and leaves herdr's prefix scheme intact for the rest.

## 9. Companion layout

**Question.** How is the companion window laid out, and how does it open from the tray?

**Facts.**
- 0392: the companion holds the canvas, Presence and voice, dictation (Right Ctrl), today, tray, autostart, pollers, events; the boss chat and threads now live in herdr.
- Presence states: still when idle; echo (1.4 s) while he holds push-to-talk; orbit (1.8 s) while the boss works; breath (1.1 s) while Jocasta speaks (`DESIGN.md:252`). 0370: Jocasta left, canvas right; today covers Jocasta's column.
- whiteboard's canvas mounts with `mountReviewCanvas(container, content, ui)` and has only dark/light themes (`whiteboard-src/packages/review/app/src/desktop-entry.tsx:155-200`); its surface colours read `--vscode-*` custom properties with fallbacks (`src/global.css:42-50`) while `--accent` is fixed `#5b7cff` (`global.css:63`), so JARVIS colours come from host CSS variables [INFERENCE: `--accent` needs a scoped override, no source patch].
- Claude Code desktop and Superset keep one sidebar of sessions with status and put tools beside it (https://claude.com/blog/claude-code-desktop-redesign, https://docs.superset.sh/agent-status).

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| Rail + canvas (recommended) | Left rail (~300 px): Presence and voice state, greeting, today (Needs you / Landed / Underway / Next), dictation hint; right: the canvas. Close hides to tray. While hidden, Right Ctrl shows a small capsule (listening -> sent). | Keeps 0370's "Jocasta left, canvas right" without the parts herdr now owns; voice always has visible feedback. | The rail takes width from the canvas. | record 0370; `DESIGN.md:252`; `desktop-entry.tsx:155` |
| Canvas first | The whole window is the canvas; a 40 px top strip carries the Presence mark, voice state and today's figures; today opens as a sheet. | Most room for diagrams and code. | The Presence shrinks to a mark; greeting and digest one click away. | DESIGN.md `status-strip`, `presence-hud` |
| Presence window + canvas window | A small Presence window (~360x480: greeting, voice, today); the canvas opens in its own window when the boss calls `show`. | Tiny footprint; canvas only when needed. | Two windows to manage; the canvas window jumps in. | record 0393 (`show(review)`) |
| Tray + capsule only | No main window until something is shown; a floating capsule shows voice states; today opens from the tray menu. | Nothing on screen when idle ("quiet when idle"). | No ambient digest; the Presence is reduced to the capsule. | PRODUCT.md principle 3 |

**Mocks.**

*Rail + canvas (recommended)*

```text
Rail + canvas
┌ ◎ JOCASTA ──────────────────────────────────────── 14:32  ─ ▢ × ┐
│  ◎  Good afternoon.        │ Canvas · acme-api · export v2   Tour │
│     Three at work. One     │ ┌──────┐   ┌───────┐   ┌─────────┐  │
│     would like a word.     │ │Client│──▶│Gateway│──▶│Exporter │  │
│                            │ └──────┘   └───────┘   └─────────┘  │
│  Today                     │      │ POST /export?v=2             │
│  Needs you  export v2      │      ▼                              │
│  Landed     add retry      │ ┌────────┐                          │
│  Underway   flaky checkout │ │ Schema │  41 export function v2(… │
│  $4.82 today · 62% left    │ └────────┘                          │
│                            │                                     │
│  hold Right Ctrl to talk   │                                     │
└────────────────────────────┴─────────────────────────────────────┘
 tray: left-click show/hide · right-click: Open, Today, Mute voice, Quit
 hidden + Right Ctrl:  ┌ ◎ listening… ┐ -> ┌ ◎ sent to the boss ┐
```

*Canvas first*

```text
Canvas first
┌ ◎ 3 at work · 1 awaiting you · $4.82 today   ◖ voice idle   Today ┐
│ Canvas · acme-api · export v2                               Tour  │
│ ┌──────┐   ┌───────┐   ┌─────────┐                                │
│ │Client│──▶│Gateway│──▶│Exporter │                                │
│ └──────┘   └───────┘   └─────────┘                                │
│     │ POST /export?v=2                                            │
│ ┌────────┐                                                        │
│ │ Schema │   41 export function v2(…                              │
│ └────────┘                                                        │
└───────────────────────────────────────────────────────────────────┘
```

*Presence window + canvas window*

```text
Presence window + canvas window
┌ ◎ JOCASTA ─────────┐      ┌ Canvas · export v2 ───────────────┐
│   ◎                │      │ ┌──────┐   ┌───────┐              │
│ Good afternoon.    │      │ │Client│──▶│Gateway│              │
│ One would like a   │      │ └──────┘   └───────┘              │
│ word.              │      │                                   │
│ Needs you export v2│      └───────────────────────────────────┘
│ $4.82 today        │
└────────────────────┘
```

*Tray + capsule only*

```text
Tray + capsule only
                            ┌ ◎ listening… ┐
                            └──────────────┘
 tray menu:  Open canvas · Today · Mute voice · Quit
 the canvas window opens only when the boss calls show(review)
```

**Recommendation: Rail + canvas.** It keeps the layout he locked in 0370 minus what moved to herdr, gives the Presence and today a permanent place, and the capsule covers dictation when the window is hidden.

## 10. Triage of his UI suggestions

**Question.** Is this triage of the UI tools from his list right?

**Facts.**
- 0397: UI suggestions are triaged in U; none is built now. His list: `local://grilling-2026-10-13.md` lines 63-122.
- "now" means the idea shapes a U decision above (config only); nothing is installed or built.

**Triage.**

| Tool | Verdict | Idea | Source |
|---|---|---|---|
| Superset | now | a thread row that says "review ready" with diff stats (herdr `state_labels` + `$tokens`); later: one-file focus mode in review | https://docs.superset.sh/agent-status, https://docs.superset.sh/diff-viewer |
| Emdash | later | best-of-N (two threads race one brief) and issue intake; no: its Electron IDE shell | https://emdash.com/docs/diff-view, https://docs.emdash.sh/ |
| Crystal / Nimbalyst | later | compare two worktrees' approaches side by side; nothing new now beyond herdr worktrees | https://github.com/stravu/crystal |
| agent-deck | now | "waiting sessions in the status bar" = the HUD's "1 awaiting you" (item 3); later: a per-project cost view | https://github.com/asheshgoplani/agent-deck (README "Notification Bar", "Search") |
| Claude Squad | no | tmux + worktree manager; herdr covers it (pause/checkout idea later at most) | https://github.com/smtg-ai/claude-squad |
| Toad | no | ACP front-end; no native Windows ("Compatibility"), and omp is the harness | https://github.com/batrachianai/toad |
| cmux | now | "latest notification text per workspace" = the blocked question in the sidebar row (item 2); no: macOS-only, GPL | https://github.com/manaflow-ai/cmux (README "Notification rings") |
| Zed | later | archive a thread when its branch merges, threads grouped by project; herdr workspaces already group | https://zed.dev/docs/ai/parallel-agents |
| ralph-tui | later | subagent tree / dashboard view; omp's agent hub covers it now; no loop runner | https://github.com/subsy/ralph-tui (README "TUI Keyboard Shortcuts") |
| Zellij | no | herdr is the multiplexer; its floating panes = herdr popups | https://zellij.dev/features |
| lazygit | now* | a herdr popup key `prefix+alt+g` (herdr's own example); *only if he installs lazygit (install needs his OK) | herdr `configuration.mdx:194-202` |
| delta | no | hunk is the review surface; delta as git pager is his personal choice | https://github.com/dandavison/delta |
| Difftastic | later | syntax-aware diff idea; whiteboard's semantic diff is the canvas-side answer | https://github.com/Wilfred/difftastic |
| Claude Code desktop | later | side chat that doesn't feed the main thread; auto-archive on PR merge | https://claude.com/blog/claude-code-desktop-redesign |
| opencode desktop | later | a toggle for the review panel (users asked for it); our zoom covers it | https://github.com/anomalyco/opencode/issues/9546 |

**Options.**

| Option | What | Pros | Cons | Evidence |
|---|---|---|---|---|
| Accept as proposed (recommended) | The table below: 4 ideas shape items 2, 3 and the lazygit popup; the rest wait in appendix A or are dropped. | Every idea has a home; nothing new is built. | — | record 0397 |
| Now items only | Keep the four "now" ideas; drop every "later" row. | Shorter appendix. | Loses best-of-N, archive-on-merge, cost view for good. | record 0397 |
| Everything after v1 | No idea shapes U; all rows wait until after v1. | U stays minimal. | Items 2 and 3 lose their reference designs; no lazygit key. | record 0397 |

**Mocks.**

*Accept as proposed (recommended)*

```text
Accept as proposed
Superset             now    a thread row that says "review ready" with diff stats (her
Emdash               later  best-of-N (two threads race one brief) and issue intake; n
Crystal / Nimbalyst  later  compare two worktrees' approaches side by side; nothing ne
agent-deck           now    "waiting sessions in the status bar" = the HUD's "1 awaiti
Claude Squad         no     tmux + worktree manager; herdr covers it (pause/checkout i
Toad                 no     ACP front-end; no native Windows ("Compatibility"), and om
cmux                 now    "latest notification text per workspace" = the blocked que
Zed                  later  archive a thread when its branch merges, threads grouped b
ralph-tui            later  subagent tree / dashboard view; omp's agent hub covers it 
Zellij               no     herdr is the multiplexer; its floating panes = herdr popup
lazygit              now*   a herdr popup key `prefix+alt+g` (herdr's own example); *o
delta                no     hunk is the review surface; delta as git pager is his pers
Difftastic           later  syntax-aware diff idea; whiteboard's semantic diff is the 
Claude Code desktop  later  side chat that doesn't feed the main thread; auto-archive 
opencode desktop     later  a toggle for the review panel (users asked for it); our zo
```

*Now items only*

```text
Now items only
Superset             a thread row that says "review ready" with diff stats (her
agent-deck           "waiting sessions in the status bar" = the HUD's "1 awaiti
cmux                 "latest notification text per workspace" = the blocked que
lazygit              a herdr popup key `prefix+alt+g` (herdr's own example); *o
```

*Everything after v1*

```text
Everything after v1
Superset             later
Emdash               later
Crystal / Nimbalyst  later
agent-deck           later
Claude Squad         later
Toad                 later
cmux                 later
Zed                  later
ralph-tui            later
Zellij               later
lazygit              later
delta                later
Difftastic           later
Claude Code desktop  later
opencode desktop     later
```

**Recommendation: Accept as proposed.** The "now" ideas cost only config and line up with items 2-3; the rest stay findable in appendix A.

## Editors demo

Same 40 lines of `src/shared/yaml.ts` and the same 3-hunk diff in the JARVIS palette, details and measurements in `docs/research/editors-demo/README.md`:

| | Monaco 0.57.0 | shiki 4.5.0 + @pierre/diffs 1.5.2 |
|---|---|---|
| Code | `editors-demo/monaco-code.png` | `editors-demo/pierre-code.png` |
| Diff | `editors-demo/monaco-diff.png` | `editors-demo/pierre-diff.png` |
| Loaded for the diff | 3.6 MB (with its diff worker) | 0.73 MB |
| Shipped | 3.6 MB in 7 files | 10.8 MB in 400 files (all shiki grammars; trimmable) |
| Highlighting | Monarch: function names stay plain | TextMate: function names lit cyan, as the canvas today |
| Built in | find, folding, selection, diff worker | rendered HTML; find, viewed marks and file tree written by Jocasta |

Recommendation (0393's default): Monaco, because whiteboard's canvas asks its host for find, active/collapsed ranges and a full diff with viewed marks, which Monaco answers built in; Pierre looks closer to hunk and loads a fifth of the bytes, but those host features would be Jocasta code.

## After the walkthrough

- One record per answer (next free numbers), each citing this file and the item's facts.
- Items 1-8 land in `config/herdr.toml` (theme, sidebar rows, tab bar, toast, keys) and in F2's `herdr.ts` (metadata reports, split for hunk, boss events); item 1 also adds the Windows Terminal fragment and `config/omp-theme-jocasta.json` to `jocasta setup`'s diff.
- Item 9 and the editors demo set the companion page and the canvas bridge (F3); `DESIGN.md` is rewritten for the decided look.
- Item 10 rows go to plan appendix A (0397).
