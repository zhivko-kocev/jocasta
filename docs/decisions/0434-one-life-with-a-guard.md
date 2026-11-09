# 0434 Jarvis opens and closes as one, with a guard

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
`jarvis` opens herdr and the companion together. What does closing do?

## Facts
Verified 2026-10-14.
- Today closing the terminal only detaches herdr's client; herdr's server, the boss and thread panes keep running; the companion's × hides it to the tray (0253); Graphiti and FalkorDB keep running in Docker; nothing stops them all at once.
- Sessions are saved by omp as they go; `omp --continue` resumes the boss and each thread where they were.
- Closing a terminal tab gives the process in it a few seconds (Windows console close event), not time to ask a question there.

## Options
| Option | Pros | Cons |
|---|---|---|
| One life, with a guard | One mental model; nothing is cut off unasked | A question when threads are working |
| One life, no question | Simplest | A running turn is cut off |
| Keep background, add `jarvis stop` | Threads survive a closed terminal | Two steps to stop |

## Recommendation
One life, with a guard.

## Choice
`jarvis` opens the companion and herdr's session `jarvis` in that terminal. Closing the terminal tab, quitting herdr's client or quitting the companion (tray Quit or the window's ×) stops everything Jarvis runs: the companion and its whiteboard server, herdr session `jarvis` with its panes, and the Graphiti and FalkorDB containers (data kept). If any thread is working or awaiting him, the companion asks first: "Stop N working threads? Their sessions are saved." with [Stop everything] and [Keep running in background]; keeping leaves everything running and the companion in the tray. The next `jarvis` resumes the boss and threads (`--continue`). His other herdr sessions and omp windows are never touched.

## Reason
"One life, with a guard".

## Consequences
- Amends 0253 (the window's × hides to the tray) and 0392 (herdr runs on after the terminal closes); 0431's herdr restart never runs during a stop.
- Amended by 0438: the question counts working and awaiting threads apart.
- Amended by 0494: a plan that awaited him comes back after a reopen through omp's `/plan-review`.
- Amended by 0518: a herdr server that dies under `jocasta`'s terminal is no longer a close; `jocasta` waits for the companion to start it again and attaches its client again.
- Amended by 0519: the close question is Jocasta's own window in the companion's look, and the service runs on while it is open.
- Amended by 0549: no [Keep running in background]; the question offers [Stop everything] and [Cancel], and everything Jocasta started stops with it.
