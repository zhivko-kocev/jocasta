# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users
One user: the owner, an engineer, at a Windows desktop. His work happens in herdr (session `jocasta`): the boss is an omp pane in workspace "Jocasta", each project a workspace, each thread a worktree workspace running omp (0388, 0392, 0412). The companion window stays open beside it all day: he glances at the rail to see who is at work and what waits on him, holds Right Ctrl to talk to the boss, hears Jocasta answer, and reads what agents draw on the canvas.

## Product Purpose
The companion is Jocasta's presence outside the terminal (0392): Presence and voice, dictation, today, toasts, the canvas, and the Jocasta service behind them (pollers, events, the Graphiti proxy, the session watcher). It succeeds when one glance tells him what is running, what waits on him and what it costs, and one click lands him on the pane in herdr.

## Positioning
Not a second place to run work: herdr and omp are the work surface; the companion greets, keeps today, speaks, listens and shows.

## Operating Context
- Electrobun desktop app with the system webview (WebView2 on Windows), strict CSP, offline: every font and asset is local (0367, 0368).
- Layout (0409, 0428, 0439): a rail on the left (the Presence and voice state, the greeting, today's Calendar / Awaiting you / Landed / Underway / Next, the day's spend, toasts, the dictation hint) and whiteboard's canvas in the middle. With a second display the window is the dashboard there, borderless over its work area; the dashboard window, from 1200 CSS px wide, has a third column on the right: the clock and weather, the news (brutalist.report tech and gaming), today's activity and the machine with Jocasta's services (0429, 0439); the laptop window never has it. The window's × closes Jocasta as one, asking first while threads are working or awaiting him; Keep running in background hides it to the tray (0434, 0438). While hidden, Right Ctrl shows a small capsule (Listening… → Sent to the boss).
- Thread state, the boss's conversation and every question live in herdr's panes; a click on a row or toast focuses that pane through herdr (0407). Links open in the system browser, never in the window (0368).
- The canvas is whiteboard's (0393); its code peeks, diffs and source panel are Monaco on Tokyo Night (0411).
- Motion only for state changes; idle CPU and GPU near zero (0258).

## Capabilities and Constraints
- One look: JARVIS, composed (0369), on Tokyo Night (0401), the palette herdr and omp also run, so the window and the terminal read as one product. It reaches the rail, the dashboard's column, toasts, the capsule and the canvas's Monaco theme; quiet text is lifted to 4.5:1 (0432).
- Fonts: Mona Sans and Geist Mono, bundled unmodified as woff2 under the OFL.

## Brand Commitments
- Name: Jocasta.
- The repo is public: placeholder names only in any shipped content (`acme`, `client-a`, `example-org`).

## Product Principles
1. State first: at work, awaiting him, spend and voice state readable in under a second.
2. Never block him: talking to the boss works whatever runs, with the window shown or hidden.
3. Quiet when idle: nothing moves unless something changed.
4. herdr and omp stand alone: the companion adds presence and a canvas, never hides or wraps a pane.
