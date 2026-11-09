# 0251 Frontier's UI surface

- Status: decided
- Kind: product
- Phase/slice: 4/4.1, 4.3
- Date: 2026-10-08

## Question
Where does Frontier's UI live?

## Facts
- Verified 2026-10-08: Bun 1.4.2 has `Bun.serve`; Edge is installed at `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`; `msedge --app=<url>` opens a window without address bar, tabs or menus (Chromium's app mode).
- Probed on this machine 2026-10-08 with his Edge already running (startup boost), so the numbers are rough. In the first run a window matching the probe appeared after 0.7 s, but the memory count matched no processes. In two later runs the window wasn't found by title within 15 s, and the processes new since launch numbered 2, with about 36 MB private memory (77–81 MB working set). This likely understates a cold start, because the already-running Edge processes were shared. The comparison that holds either way: reusing the installed Edge ships no second browser engine, unlike Electron.
- herdr's docs: Windows Terminal doesn't support the Kitty graphics path (https://herdr.dev/docs/windows-beta/). whiteboard is a desktop app on a vendored VS Code core (Electron) (https://github.com/devdotfast/whiteboard).
- His first answer asked how whiteboard works and whether the window has browser bars; his second: "okay then the recomended, it will be fast and wont need much resources right?"

## Options
| Option | Pros | Cons |
|---|---|---|
| A local web UI served by Jarvis, opened as an Edge app window | No new toolchain; HTML, SVG and CSS for animations and diagrams; reachable from a phone later | Depends on Edge (or another Chromium) |
| A desktop app (Tauri or Electron) | Native window and tray | A new toolchain and a second build |
| A terminal TUI (pi-tui) | The plan's design | Text-only animations; no real diagrams |

## Recommendation
The local web UI.

## Choice
A local web UI served by Jarvis (Bun) and opened as an Edge app window with no browser bars.

## Reason
"okay then the recomended, it will be fast and wont need much resources right?"

## Consequences
- Plan 4.3's terminal layout becomes a web layout, grilled at 4.3.
- The port, the browser fallback when Edge is missing, and who may connect are 4.1's questions.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
