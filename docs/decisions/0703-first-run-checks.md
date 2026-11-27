# 0703 First-run checks

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J8 (issue #8)
- Date: 2026-11-26

## Question
On a fresh Linux or macOS machine Jocasta starts even when what it needs is missing: omp not installed or not signed in, an old git, no `notify-send` or `xdg-open`, a terminal that doesn't report Alt and Shift. Each shows up later as a silent failure (no notification, no brief, keys that do nothing, pushes to main unasked). Does Jocasta tell him at start, and how?

## Facts
- `notify-send` missing turns notifications off with no message (`own_surface.rs`); links and briefs open through `xdg-open` (`link.rs`).
- `omp usage` (without `--json`) exits 1 with `No credentials found. Run \`omp\` and use /login to add accounts.` when omp has no login (omp 19.2.1); `omp usage --json` exits 0 with empty `reports` then.
- Setup already notes a git older than 2.54: the pre-push hook Jocasta gives every pane needs it (0613).
- The shell asks the terminal for the kitty keyboard protocol at start (crossterm's `supports_keyboard_enhancement`); without it Alt and Shift combinations don't reach Jocasta. On Windows the console input reports them without it.
- The README covered Windows only, and its Linux upgrade line was stale (issue #8).

## Options
| Option | Pros | Cons |
|---|---|---|
| Check once at start, in the background; one notification each and a line in `today`'s header | Nothing slows the start; the header keeps saying it while the run lasts | Fixed only after a restart shows it gone |
| Check in `jocasta setup` only | Nothing at run time | He may never rerun setup; the terminal is the one `jocasta` runs in |
| Refuse to start | Can't be missed | Blocks a run that mostly works |

## Recommendation
Check once at start, in the background.

## Choice
His answers of 2026-11-26 and the approved plan:
- When the service has started, a background task checks once, and each problem it finds goes out as one notification and stays in `today`'s header (in red) for the rest of the run. The texts:
  - omp not on PATH: `omp isn't on PATH: install omp, run omp, then /login`;
  - omp not signed in (`omp usage` fails saying `/login` or `No credentials`): `omp isn't signed in: type /login in the boss pane`;
  - git older than 2.54 or missing: `git <version> is older than 2.54: install git 2.54 or newer, or pushes to main, master and develop go unasked`, and `git isn't installed: …` with the same fix;
  - Linux only: `notify-send isn't installed: install libnotify, or Jocasta's notifications stay off`, `xdg-open isn't installed: install xdg-utils, or briefs and links won't open`;
  - Linux and macOS: `this terminal doesn't send Alt and Shift keys Jocasta needs; use foot, kitty, WezTerm or Ghostty`.
- The terminal's answer is asked once, before the service starts, and used by both the shell and the check.
- README and manual gain a Linux and macOS install and first-run section with placeholder examples (`~/code/acme-api`); Windows-only sentences say so. The README's install line for Linux and macOS is the install script until the first release, when 0691 makes it `mise use -g github:zhivko-kocev/jocasta`.

## Reason
Each missing piece is named once, where he looks, with the step that fixes it, and the start is no slower.

## Consequences
- New `server/first_run.rs`: the facts, the texts, and the task `Service::start` spawns; `ServerOptions.first_run` (whether the terminal reports keys; unset in tests, so tests run no check).
- `Hud.problems` carries the texts to `today`'s header.
- `setup::git_too_old` is shared by setup's note and the check.

## Note (2026-11-27)
In the VM, a fresh install's GitHub poller sent the notification `Poller github failed: … gh auth login` at every start: a `gh` without a login is a fresh machine's state, like the checks above, not a failure. When a GitHub round fails with `gh`'s sign-in text (`gh auth login`), the poller says nothing as a notification and `today`'s header carries one line, `gh isn't signed in: run gh auth login to see review requests and CI here`, until a round finds `gh` signed in (`Day::problems`, read into `Hud.problems` after the checks' own).
