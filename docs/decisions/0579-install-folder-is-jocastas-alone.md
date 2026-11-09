# 0579 The install folder is Jocasta's alone

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V3
- Date: 2026-11-09

## Question
The review found that the installers don't check they own the install folder, so an upgrade or uninstall can delete unrelated files, and that uninstall says done while open omp sessions still run programs from it. How do install and uninstall treat the folder?

## Facts
Verified 2026-11-09 by the review (0573), reports SEC-LOC (SEC-LOC-2) and SET (SET-8, SET-9).
- `install.sh` extracts into whatever `JOCASTA_INSTALL_DIR` names with no check and writes `install.json` naming it (`scripts/install.sh:66-80`); it upgrades in place and keeps stale files.
- `install.ps1` refuses a non-empty folder only when it holds neither `jocasta.exe` nor `install.json` (:34-36), then mirrors the release over it with `robocopy /MIR`, purging every file the release lacks.
- Uninstall removes a folder whose `install.json` names it whole (`src/setup/uninstall.ts:178-181`).
- Migration 1 puts `<install>\jocasta.exe mcp` and whiteboard's `node.exe … mcp` in his global omp config, so every omp session holds files in the folder; uninstall checks only the service, and the remover's `rmdir /s /q` output is discarded.

## Options
| Item | Options (pros / cons) |
|---|---|
| SEC-LOC-2 ownership | 1. strict check in both installers plus manifest-based uninstall (uninstall never takes more than it shipped / install.json grows) · 2. installers only (existing shared folders stay dangerous) · 3. uninstall asks y/N with a list (relies on reading it) |
| SET-9 install.sh | Same refusal as install.ps1, extract beside and swap (no stale files) · as now |
| SET-8 held files | A. refuse with the list, the remover retries (clear / he closes sessions) · B. retry only (silent) · C. end the processes (breaks his sessions' tools) |

## Recommendation
SEC-LOC-2 option 1 with SET-9; SET-8 A.

## Choice
- SEC-LOC-2 option 1 + SET-9: both installers refuse a non-empty folder unless its `install.json` names that same folder (install.ps1's `jocasta.exe` escape goes; install.sh gets the same drive-root and non-empty check). `install.json` records the release's top-level entries (`"entries": [...]`); uninstall removes those entries and `install.json`, then the folder only if it is empty (old installs without `entries`: the whole folder only when every entry is a known release name). robocopy gets `/XJ`. install.sh extracts to `$dir.new-<stamp>` and swaps (the old one moved aside, the new one in, the old one removed). Refusal: `jocasta install: /home/x/.local/bin holds files that aren't Jocasta's; choose an empty folder for JOCASTA_INSTALL_DIR.`
- SET-8 A: before asking, uninstall lists the processes whose executable is under the install folder and refuses: `jocasta: omp sessions still run programs from <install>: jocasta.exe mcp (pid 4120), node.exe whiteboard mcp (pid 5332). Close them, then run jocasta uninstall again.` The detached remover retries for a few minutes as a safety net.

## Reason
"Option 1" for SEC-LOC-2, with SET-9's install.sh fixes; "A" for SET-8.

## Consequences
- Amends 0049 (the install folder), 0468 (uninstall removes only what the release shipped) and 0484 (the installers' checks).
