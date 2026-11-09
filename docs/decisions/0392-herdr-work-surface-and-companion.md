# 0392 herdr is the work surface, with a companion

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
What is Frontier when herdr owns the terminals?

## Facts
Verified 2026-10-13.
- herdr (`herdrdev/herdr`) 0.9.4 (2026-10-13), Apache-2.0, Rust; Windows is GA through ConPTY, `herdr terminal attach` is unsupported there. It has an NDJSON socket API on a named pipe (`workspace.*`, `tab.create`, `pane.*`, `agent.*`, `worktree.*`, `notification.show`, `events.subscribe`), env `HERDR_SESSION` and `HERDR_CONFIG_PATH`, and `herdr integration install` supports OMP on Windows.
- Without patches herdr gives workspaces, tabs and panes with agent states in its sidebar, custom sidebar rows from `pane.report_metadata`, tab-bar status from a command, notifications with sounds, persistence, SSH attach and worktrees.
- A terminal can't draw whiteboard's canvas, play kokoro on WebGPU or hook Right Ctrl.
- Frontier today: ~13K lines (server 4.0K, page 4.6K, canvas, dictation 0.7K, host 0.5K).

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr is Frontier + a companion window | The vision's TUI; herdr's UX for the work; no pane-stream patch; remote-ready | Retires the desktop work surface; two surfaces |
| herdr under the desktop Frontier | One surface, the composed look everywhere | herdr's UX hidden; a Rust pane-stream patch rebased weekly |
| herdr is Frontier, no GUI | One terminal surface | The canvas isn't merged; no Presence |

## Recommendation
herdr is Frontier + a companion window.

## Choice
herdr (installed by its official installer, stable channel) is the work surface in session `jarvis` with config `~/.jarvis/agent/herdr.toml`: workspace "Jarvis" holds the boss pane, one workspace per project, one tab per thread. The boss is omp with the boss manual appended and `jarvis mcp --boss` tools. Everything that talks to herdr is one module (principle 7). The Electrobun app becomes the companion: canvas, Presence and voice, dictation (Right Ctrl), today, tray, autostart, pollers, events, the Jarvis service. Jarvis's PTY host, thread tabs and in-process boss retire. The look comes from the UX pass (0395).

## Reason
"tweak herdr fork to achieve what we have imagined"; "isnt herdr as the frontier better"; "yest the recommendeed but the colors and that stuff we do from 0 or maybe do the ui like herdr we still can decide".

## Consequences
- `src/frontier/server/herdr.ts` is the one herdr module.
- herdr is used unforked unless a UX record needs what its config can't express (0383).
- Amended by 0412: threads are herdr worktree workspaces grouped under their project, not tabs.
- Amended by 0434: closing the terminal, herdr's client or the companion stops everything Jarvis runs, asking first when threads are working.
- Amended by 0495: in a session Jocasta just created, herdr's untouched default workspace is closed.
- Amended by 0552: no companion app; the service is a plain Bun process beside herdr, diagrams open in the browser.
