# 0049 Install scripts

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
How is Jarvis installed?

## Facts
- The repo is public (0035), so release assets download without auth through `https://github.com/<owner>/jarvis/releases/latest/download/<asset>`.
- A release is a folder (0044). Pi finds its files next to `process.execPath`, so they stay beside the exe.
- Whether `process.execPath` resolves a symlink to the real exe on macOS is unverified.

## Options
| Option | Pros | Cons |
|---|---|---|
| `install.ps1` and `install.sh` over HTTPS with checksum verification | No gh dependency | — |
| Through `gh release download` | Works for a private repo | Needs gh and auth on every machine |

Unix location, asked when the build reached it:

| Option | Pros | Cons |
|---|---|---|
| Folder in `~/.local/share/jarvis/`, plus a wrapper script `~/.local/bin/jarvis` that `exec`s it | `~/.local/bin` stays clean; no symlink question | One extra file |
| Same folder, symlink in `~/.local/bin` | No script | macOS behavior unverified |
| Same folder, appended to PATH in the shell rc | No wrapper | Edits shell rc files |

## Recommendation
HTTPS with checksum verification; on Unix, the folder plus a wrapper script.

## Choice
`scripts/install.ps1` and `scripts/install.sh` download the latest release over HTTPS, verify its checksum, and honor `JARVIS_INSTALL_DIR` as the install folder.
- Windows: the folder is `%LOCALAPPDATA%\Programs\jarvis\`, added to the user's PATH.
- Unix: the folder is `~/.local/share/jarvis/`, and the script appends it to PATH in the shell rc file: `~/.zshrc` when `$SHELL` is zsh, `~/.bashrc` when it is bash, `~/.profile` otherwise. The line is marked `# jarvis` and is added only once.

## Reason
Zhivko changed the Unix location from the wrapper script to the shell rc on 2026-10-04.

## Consequences
- The first answer was `gh release download` while the repo was private; it changed when the repo went public.
- The Unix location changed from `~/.local/bin` once packaging (0044) made a release a folder.
- Differs from the recommendation (a wrapper script in `~/.local/bin`); the first answer was the wrapper, changed in place (0014).
- Amended by 0484: the expected SHA-256 comes from the repository at the release's tag (`checksums-<tag>`), never from the release itself.
