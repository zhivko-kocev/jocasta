# 0549 Nothing runs without Jocasta: no Keep in the background, autostart off by default, paused work resumes on the next start

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V3
- Date: 2026-11-08

## Question
He doesn't want processes running when Jocasta isn't active: "i want everything explicit when jocasta runs herdr and companion run and when they exit everything is paused". What changes?

## Facts
Verified 2026-11-08.
- 0434: closing the terminal, quitting herdr's client or quitting the companion stops everything (companion, whiteboard server, herdr session, containers), but when threads work or await him the question offers [Keep running in background], which leaves everything running with the companion in the tray.
- 0367: autostart at logon in v1 (`frontier.autostart`, on by default, a Windows Run key).
- Graphiti and FalkorDB start on use and stop after 15 idle minutes; their data stays in volumes. Docker Desktop is his, not Jocasta's.
- omp's `--restore` resumes a session and goes on with what its last process left unfinished (the interrupted turn and its subagents); `--continue` reopens it idle.
- The harness-OS layer adds a relay (0536), an index daemon and dev servers (0542).

## Options
| Item | Options (pros / cons) |
|---|---|
| Exit | Everything stops, no Keep (explicit / no unattended work after he closes it) · As today |
| Autostart | Removed · An option, off by default · On, as today |
| Resume | Paused work resumes (`--restore`) · Every thread reopens idle (`--continue`) |

## Recommendation
Everything stops; autostart removed; paused work resumes.

## Choice
- Jocasta runs while herdr's `jocasta` session and its companion run. Closing either stops everything Jocasta started: herdr's session and panes, every omp it launched, the pollers, the containers, whiteboard, hunk, the relay, the index daemon and dev servers. The close question keeps its count of working and awaiting threads, with [Stop everything] and [Cancel]; there is no Keep.
- Minimizing or hiding the companion's window doesn't stop anything; push-to-talk works while Jocasta runs.
- Autostart stays as `frontier.autostart`, off by default.
- On the next start, threads that were mid-turn continue that turn (`omp --restore`), the others reopen idle; GitHub, Azure DevOps and the calendar are read again, and reminders and events that came due while it was off show once.

## Reason
"Everything stops, no Keep", after asking "does everything stops make sense ?"; "Option, default off"; "Paused work resumes".

## Consequences
- Differs from the recommendation on autostart (an option, off).
- Amends 0434 (no Keep), 0367 (autostart off by default; setup removes the Run key unless he turns it on) and 0521 (how a thread's omp comes back on start).
- Every new background process the harness-OS layer adds stops with Jocasta.
- Amended by 0552: Jocasta is herdr's `jocasta` session plus the Bun service it starts; there is no app or tray.
- Amended by 0557: `frontier.autostart` goes; he starts `jocasta` himself.
- Amended by 0563: closing the terminal while threads work reopens it on the same herdr session and asks the close question in the popup.
