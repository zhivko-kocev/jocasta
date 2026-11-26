# 0701 Linux guard shapes and fenced folders

- Status: v1 (built 2026-11-26)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J1 (#10, #11)
- Date: 2026-11-26
- Amends: 0594 (Linux gets its own fence), 0450 and 0580 (Linux shapes)

## Question
On Linux the guard denies none of the machine's or the session's ends (`shutdown now`, `systemctl reboot`, `dd … of=/dev/sda`, `mkfs`, `wipefs`, `loginctl terminate-user`), and omp's fence covers only the shipped `~/.ssh ~/.aws ~/.azure ~/.docker`, so agents read the keyring, GnuPG's keys, gh's token and `/etc` unasked. What does Linux add?

## Facts
- `config/guard.yml` had one shutdown shape, Windows' `*shutdown /*` (issue #11).
- `src/setup/setup.rs` adds `%SystemRoot%` and `%ProgramFiles%` to `tools.restrictedFolders` on Windows only (0594, issue #10).
- omp matches each segment of a compound command, anchored; a rule without a leading `*` matches a segment that starts with it.

## Options
| Option | Pros | Cons |
|---|---|---|
| Deny the shapes; fence the five folders on Linux | No silent ask under yolo; secrets behind a question | A few false denials (`dd … of=/dev/null`) |
| Prompt instead of deny | Fewer refusals | A prompt is allowed under yolo (0450) |

## Recommendation
Deny and fence.

## Choice
His answers of 2026-11-26 and the approved plan:
- `config/guard.yml` denies `shutdown`, `shutdown *`, `poweroff`, `poweroff *`, `reboot`, `reboot *` (each also after `sudo`), `systemctl … poweroff|reboot|halt|suspend|hibernate`, `dd … of=/dev/…`, `mkfs*`, `wipefs`, `loginctl terminate-*|kill-*` and `pkill -KILL -u` (and `pkill -9 -u`). The bare commands are matched at a segment's start, so a commit message that names them still runs. 228 deny rules, then 55 prompt rules.
- On Linux setup adds `~/.local/share/keyrings`, `~/.gnupg`, `~/.config/gh`, `/etc` and `/boot` to `tools.restrictedFolders`, shown in setup's diff like Windows' folders. `/usr` stays out (0594's reason).

## Reason
These end his machine or his session, and several need no root; the fenced folders hold secrets in plain files.

## Consequences
- Each shape has a line in the guard's Rust and Bun tests, with safe neighbours that still run.
- An existing Linux install sees the fence as a change to the omp snapshot in `jocasta setup`.
