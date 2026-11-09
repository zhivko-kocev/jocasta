---
name: diagnose
description: Explain why Jocasta misbehaved from its own evidence (its log and the service's state), changing nothing. Use in the pane `jocasta diagnose` opens.
---

# Diagnose Jocasta

`jocasta diagnose` opened this pane read-only (0472): plan mode, no edits, read and search tools only. It attached two files to its first message, both in this pane's folder:

- `state.md`: the service's state when the pane opened: versions, the service (up, busy, closing or down), herdr's session and its panes, setup's pending changes, and where the logs are.
- `recent.log`: the newest lines of Jocasta's log. The full logs are `<data>/logs/jocasta-YYYY-MM-DD.log`, one per day, seven days kept; `state.md` names the folder.

## Method

1. Start from what went wrong in his words. If he hasn't said, ask once: what he did, what he expected, what happened instead, and roughly when.
2. Read `state.md` first: it rules causes in or out (a service that is down, a herdr session that doesn't answer, setup changes waiting).
3. In the log, find the first `WARN` or `ERROR` near that time, then read the lines before it. Each line is `<time> <LEVEL> [<scope>] <message>`; the scope (`service`, `herdr`, `graph`, `day`, …) names the part that wrote it. Older days' logs are there when the trouble started earlier.
4. Follow the evidence with read, grep and glob only: the log, `state.md`, the shipped config, Jocasta's source when it is on this machine.
5. Explain: the cause in one or two sentences, the evidence (quote the log lines with their times), how sure you are, and what he can do about it (a command he runs, a setting he changes). When the evidence doesn't settle it, say what is missing and how he could capture it.

## Never

- Change anything: no file edits, no commands, no restarts, no kills, no installs, nothing written to herdr, Docker, omp or Jocasta. Proposing a fix is the end of your work; he applies it.
- Guess past the evidence, or present a guess as the cause.
- Repeat secrets: tokens, the calendar's address, API keys, anything that looks like one in the log.
