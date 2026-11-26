# 0702 Unix runtime

- Status: v1 (built 2026-11-26)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J1 (#3, #6, #7, #15)
- Date: 2026-11-26
- Amends: 0675 (the Unix socket's folder), 0683 (the stop's deadline), 0549 (headless reviews end with Jocasta)

## Question
Four Unix gaps: an upgrade while Jocasta runs leaves `current_exe()` naming a deleted file, so panes started after it lose their config (#3); foot SIGKILLs the process group 60 s after closing, the same as Jocasta's own stop wait (#6); the weekly review's headless omp outlives Jocasta (#7); the bridge socket sits at the top of `$XDG_RUNTIME_DIR`, whose mode Jocasta changed, and killed runs leave sockets behind (#15). What does Jocasta do?

## Facts
- `install_dir()` resolved `current_exe()` on every call (`src/shared/install.rs`).
- `STOP_WAIT` was 60 s (`server/embedded.rs`); foot sends SIGHUP and SIGTERM, then SIGKILL after 60 s.
- `run_program` makes each program a process-group leader with no kill-on-exit; Linux's `PR_SET_PDEATHSIG` sends a signal to a child when the thread that started it ends.
- The socket was `${XDG_RUNTIME_DIR:-/tmp/jocasta-<uid>}/bridge-<port>.sock`, and only the same port's file was removed before binding.

## Options
| Option | Pros | Cons |
|---|---|---|
| Resolve the install once at start; one 50 s deadline; reviews run with the service's stop signal and, on Linux, `PR_SET_PDEATHSIG`; sockets in `$XDG_RUNTIME_DIR/jocasta` | Each gap closed at its cause | A pane started after an upgrade still uses the build Jocasta started from, until he restarts |
| Map `<dir>.old-*` back to `<dir>` | New panes get the new config | Mixed builds in one run |

## Recommendation
The first.

## Choice
His answers of 2026-11-26 and the approved plan:
- `entry::main` resolves the running executable and its install folder once (`install::resolve_at_start`); every later `install_dir(None)` gets that folder.
- `STOP_WAIT` is 50 s.
- The weekly big-picture review's omp runs with the service's stop signal (`ServiceContext::stop_signal`, aborted when the service stops, which ends its process group) and, on Linux, `RunOptions::end_with_jocasta` (`PR_SET_PDEATHSIG` with SIGTERM), so a killed Jocasta takes it down too.
- Bridge sockets live in `$XDG_RUNTIME_DIR/jocasta/`, else `/tmp/jocasta-<uid>`; Jocasta creates that folder with mode 0700, refuses one another user owns ("The bridge's folder … belongs to another user"), and before binding removes `bridge-*.sock` files nothing listens on.

## Reason
An upgrade, a closing window, a killed process and a leftover socket must not cost his work or leave processes running (0549).

## Consequences
- Plan appendix I: the socket path and `RunOptions::end_with_jocasta`.
