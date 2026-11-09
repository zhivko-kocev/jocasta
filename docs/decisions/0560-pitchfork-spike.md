# 0560 pitchfork isn't kept: dev servers stay plain background processes

- Status: dropped
- Kind: product
- Phase/slice: program
- Vision: V11
- Date: 2026-11-09

## Question
0542 tries pitchfork for threads' dev servers and keeps it only if it measures better than today's plain background processes. Does it?

## Facts
Observed 2026-11-09 with pitchfork 2.33.1 (through mise, scoped to `D:/jocasta-spikes/pitchfork/`) on native Windows 11 against a baseline of the same Bun server started in the background with its output in a log file; no model calls.
- Start, stop, restart, status, logs, readiness checks, port bumping and crash restarts work on Windows; commands run through `cmd.exe` (`%PORT%`, not `$PORT`). `pitchfork boot enable` would add a logon Run entry; it was never run and no Run key or task appeared.
- Token cost: status 62 bytes against a 3-byte health check; the last 20 log lines 111 bytes against 118 for `tail`; JSON output 3–8× larger; without `-q`, `--raw` and `-n` a start prints a 1.1 KB spinner and logs print their whole history.
- `pitchfork supervisor stop` left no process in under 0.6 s; a hard-killed supervisor left its dev servers running and serving until a new supervisor took them over. The baseline's children went down with it.
- Ports: it bumps to a free port and refuses a taken one in 58 ms naming the holder, where the baseline's health check reported ready against the other server; two folders with the same name share one daemon name unless a `namespace` is set.
- It adds 30–550 ms per start, a supervisor and a logging process per daemon; three crash retries are a lifetime budget.

## Options
No choice for him: 0542's rule decides (kept only if it measures better).

## Recommendation
As the choice.

## Choice
pitchfork isn't kept. Threads run dev servers as today; nothing is added for them.

## Reason
No token saving, and a supervisor that dies leaves servers running, against 0549.

## Consequences
- Carries out 0542's keep rule for pitchfork.
- Left on disk: `D:/jocasta-spikes/pitchfork/` (fixture and its `mise.toml`); pitchfork 2.33.1 uninstalled through mise; its `~/.local/state/pitchfork` folder deleted by the spike; nothing running.
