# 0594 Setup fences the key folders and Windows' own, and writes an empty secrets.yml

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V8
- Date: 2026-11-10

## Question
Two of 0593's values don't fit the shipped reference as they are. The fenced folders differ by OS, and `secrets.yml` is a file of its own, not a key. How does `jocasta setup` ship them?

## Facts
Verified 2026-11-10.
- **The reference** is one static YAML file for every OS. Migration 3 merges it into his `config.yml` and runs again whenever its text changes (`src/setup/setup.ts:509-525`, 0466).
- **A Windows path elsewhere:** omp resolves a configured folder against the call's folder (`tools/approval.ts:584`), so `C:\Windows` on macOS or Linux would mean a folder inside the project.
- **His Windows values:** `%SystemRoot%` is `C:\WINDOWS` and `%ProgramFiles%` is `C:\Program Files`. omp compares Windows folders case-folded.
- **macOS and Linux** keep their system folders owned by root. Fencing `/usr` would stop every read of a system header or a global package under it.
- **The pattern to copy:** migration 11 (`WATCHDOG.md`) writes a shipped file when it's missing and follows a newer shipped text while the file is still Jocasta's; `jocasta uninstall` removes it only then (0479).
- **The file's format:** omp reads `secrets.yml` as a YAML list. A file of comments only is not a list, so omp logs a warning at every start (`secrets/index.ts:295-302`).

## Options
The OS folders in the shipped settings:

| Option | Pros | Cons |
|---|---|---|
| OS folders on Windows only | Matches his PC; Windows' own values, so another drive works too; macOS and Linux already protect theirs | On a Mac or Linux box agents read `/etc` without asking |
| Also macOS and Linux system folders (`/etc`, `/usr`, `/bin`, `/sbin`, `/System`, `/boot`) | The same fence on every OS | Stops on reads of headers and global packages, common in coding |
| Key folders only, shipped | No per-OS code in setup | Other Windows installs don't get the OS fence |

`secrets.yml`:

| Option | Pros | Cons |
|---|---|---|
| His PC only | Setup writes nothing new; an empty file does nothing | Others don't learn the file exists |
| Setup creates it | Every install sees where to list its own names | A new file for setup to write and uninstall to know |

## Recommendation
OS folders on Windows only; `secrets.yml` on his PC only.

## Choice
- **The fence:**
  - The reference ships `tools.restrictedFolders: ["~/.ssh", "~/.aws", "~/.azure", "~/.docker"]`.
  - On Windows, setup adds Windows' own `%SystemRoot%` and `%ProgramFiles%` (`C:\Windows` and `C:\Program Files` when unset).
  - Off Windows, the list is the four key folders.
- **`secrets.yml`:**
  - A new migration writes `~/.omp/agent/secrets.yml` from `config/secrets.yml` when it's missing: the format, one commented placeholder example (`acme`), and an empty list `[]` for him to replace.
  - A file he changed is kept and never shown in a diff, because it holds his strings. One still at Jocasta's earlier text follows the new one.
  - `jocasta uninstall` removes it only while it is still Jocasta's text.

## Reason
"OS folders on Windows only"; "Setup creates it".

## Consequences
- The `secrets.yml` choice differs from the recommendation.
- Amends 0451 (setup writes `secrets.yml` when it's missing), 0466 (one more migration) and 0479 (setup writes four omp files beside `config.yml`).
- The template's text, the migration's number and its slug are the builder's, listed as built (0363).
- README's setup list and config section, and plan appendix I, list both.
