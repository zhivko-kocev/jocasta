# 0253 Frontier server lifetime

- Status: decided
- Kind: product
- Phase/slice: 4/4.1
- Date: 2026-10-08

## Question
Does the server stop when the window closes?

## Facts
- With the web UI (0251), the Bun server and the Edge window are separate processes; stopping on close needs code to notice it. V3: Frontier lives as long as the PC runs.

## Options
| Option | Pros | Cons |
|---|---|---|
| The server keeps running; `jarvis` reopens a window; Quit in the UI stops it | V3; threads and the boss carry on | A background process |
| Closing the window stops everything | Nothing in the background | Close detection; threads stop |

## Recommendation
The server keeps running.

## Choice
Now (0357): Quit asks "Close Frontier" (the default; threads keep running in the background host) or "Stop threads too" (ends the host; running threads are marked interrupted).

Originally: The server keeps running when the window closes; threads and the boss carry on. `jarvis` opens a window to the running server; a Quit action in the UI stops it.

## Reason
Not given.

## Consequences
- None.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0434: closing the terminal, herdr's client or the companion stops everything Jarvis runs, asking first when threads are working.
