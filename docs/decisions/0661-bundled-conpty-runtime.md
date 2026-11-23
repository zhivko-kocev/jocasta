# 0661 The Windows build ships Microsoft's ConPTY runtime, as herdr's does

- Status: v1
- Kind: product
- Phase/slice: fork build, after his first day on it
- Date: 2026-11-23

## Question
On his first day on the Rust build no Windows notification showed. Ship what they need, or turn them off?

## Facts
Verified 2026-11-23:
- Every toast failed in `herdr-server.log` with "Windows notification activation requires the bundled ConPTY runtime": `crates/herdr/src/platform/windows/notifications.rs` needs `<exe dir>/conpty/x64/OpenConsole.exe`, and Jocasta's build has no `conpty\` folder.
- Upstream herdr 0.9.5 ships the folder (`crates/herdr/scripts/package_windows_conpty.py`): Microsoft's NuGet package `Microsoft.Windows.Console.ConPTY` 1.24.260710001 (MIT), from the `v1.24.11911.0` release of `github.com/microsoft/terminal`, pinned by package and file SHA-256 in `crates/herdr/packaging/windows/conpty.json`, with its license and notice.
- With the folder present, herdr's patched portable-pty hosts panes on the bundled `conpty.dll`, after checking each file's hash and the exact layout; a wrong bundle stops a pane's start with a message naming `HERDR_WINDOWS_CONPTY=system`, which uses Windows' own ConPTY. Without the folder, panes use Windows' own.

## Options
- **Ship the pinned runtime as herdr does.** Pros: toasts work; panes on the newer ConPTY upstream herdr is tested with. Cons: a new shipped dependency (3 files, about 2 MB in the zip).
- **No Windows toasts.** Pros: nothing new shipped. Cons: no notification outside the terminal.

## Recommendation
Ship it.

## Choice
His answer of 2026-11-23: ship it. `mise run build` downloads the pinned package once (a cache like Node's), checks its SHA-256 and each file's against `conpty.json`, and puts `conpty\conpty.dll`, `conpty\x64\OpenConsole.exe`, `conpty\arm64\OpenConsole.exe`, herdr's `conpty\herdr-conpty.json` marker and the two notices in `THIRD-PARTY-NOTICES\` into the Windows zip; `install.ps1` installs them with the rest, renaming a held file in place as it does for every `.exe` and `.dll`.

## Reason
His answer above; herdr's own packaging, unchanged.

## Consequences
- A new shipped dependency, pinned by hash; a bump follows herdr's `conpty.json`.
- Panes on Windows run on ConPTY 1.24; `HERDR_WINDOWS_CONPTY=system` is the way back for a pane problem.
- Amends 0649 (what the Windows build ships).
