# Own shell on `alacritty_terminal`: the spike (record 0683, 2026-11-24)

The spike of phase 6 of the lightweight program: can Jocasta's own shell, a pty and `Term` from `alacritty_terminal` 0.26.0 drawn with ratatui and crossterm, replace the herdr fork? This file holds what was built, how to run it, rows 7 and 8 of 0683's checklist measured on Windows and on WSL Ubuntu, rows 1–6 as his manual procedure (verdicts empty), and what already failed or looks risky.

Verdict so far: **rows 7 and 8 pass on Windows with the system ConPTY and on WSL; they do not run on Windows with the bundled `conpty/` runtime** (a stall, section 3). Rows 1–6 need him. Nothing in the repo changed except this file; the spike is untracked under `target/spikes/own-shell/`.

## 1. What was built

A throwaway Cargo package, `target/spikes/own-shell/`, outside the workspace (an empty `[workspace]` table in its own `Cargo.toml`). About 1,400 lines in `src/`:

| File | What |
|---|---|
| `pane.rs` | one pane: `tty::new` + `Term` (kitty keyboard on, 10,000 lines of scrollback) + `EventLoop`; the listener answers the pane's own queries (`PtyWrite`, OSC 10/11 colour requests with fixed colours, text-area size) and sends `Wakeup`, title and exit to the UI thread |
| `keys.rs` | crossterm key events → bytes: legacy, kitty CSI-u by the pane's `TermMode` flags (disambiguate, event types, all keys as escape codes, associated text), SGR and legacy mouse, bracketed paste |
| `view.rs` | `Term` grid → ratatui cells (colours, attributes, wide characters, zero-width marks, selection, cursor), and the tab bar |
| `main.rs` | the window: two tabs, one pty each, raw mode, alternate screen, mouse capture, bracketed paste; a UI thread that draws only on damage (at most every 8 ms) and never draws the hidden tab; local selection and OSC 52 copy; wheel scrolls the shell's scrollback (or sends arrows in an alternate-screen app); on Windows a burst of 8 or more text key presses within 1 ms each is taken for a paste |
| `stubs.rs` | stand-ins run in panes: `stub-flood`, `stub-idle`, `stub-input` (raw-input recorder) |
| `probe.rs` | `probe` and `omp-probe`: automated parts of rows 1–3 and 6 |
| `forkdrive.rs`, `*.ps1`, `*.sh` | the measuring drivers |

Dependencies: `alacritty_terminal =0.26.0`, `ratatui =0.30.2`, `crossterm =0.29.0` (the repo lock's versions), `libc` on unix (in the repo lock). Compared with `Cargo.lock` of the repo, the spike's lock adds only what `alacritty_terminal` pulls: `arrayvec 0.7.8`, `cursor-icon 1.2.0`, `hermit-abi 0.5.3`, `home 0.5.12`, `miow 0.6.1`, `nix 0.31.4`, `piper 0.2.5`, `polling 3.11.0`, `rustix-openpty 0.2.0`, `signal-hook 0.4.6`, `uuid 1.29.3`, `vte 0.15.0`, `windows-sys 0.59.0` (the duplicate 0683 predicted: it sits beside the repo's 0.52 and 0.61.2; `cargo deny` would need the `skip`). The repo's 7-day rule applied (`.cargo/config.toml` is found from `target/spikes/`).

`Cargo.toml` carries a `[patch.crates-io]` to `vendor/alacritty_terminal`, a copy of 0.26.0 whose only change reads the ConPTY flags and the output pipe size from env vars (`OWN_SHELL_CONPTY_FLAGS`, `OWN_SHELL_PIPE`) for the experiments in section 3. With no env var it behaves as upstream (flags 0, pipe size 0). Delete the table to get the registry crate.

Two tabs, each running omp 19.1.1: yes. `own-shell-spike -- omp` starts two omp panes; `own-shell-spike -- omp --version` showed both start and print `omp/19.1.1` in the grid and then show `[exited]` in the tab. `omp-probe` (below) started a real omp and read its first screen from the grid.

### Run it

```
cd D:\jocasta\target\spikes\own-shell
cargo build --release
copy target\release\own-shell-spike.exe run-sys\            # system ConPTY (kernel32); the working variant
run-sys\own-shell-spike.exe                                  # two tabs, each `omp`
run-sys\own-shell-spike.exe -- omp --model @smol             # any other command after --
```

Keys of the spike itself: `F1`, `F2` switch tabs (also a click on the tab bar), `F12` quits. Everything else goes to the pane. Plain drag selects (Shift+drag when the program uses the mouse); releasing copies through OSC 52. `run-conpty\` is the same exe with `conpty.dll` and `x64\OpenConsole.exe` beside it, the layout of the fork's package (section 3). WSL: `wsl -d Ubuntu -e ~/own-shell-spike/target/release/own-shell-spike -- bash` (a copy built there with Rust 1.100.0, `--locked`; omp isn't installed in WSL, so use `bash`, `cat -v` or another command).

The measuring commands:

```
own-shell-spike bench-flood 50              # row 7: 50 MiB through one pane, 120x39 grid, frames formatted into a sink
own-shell-spike bench-idle 4 60             # row 8: 4 idle panes (measured from outside)
own-shell-spike probe kitty|kitty1|legacy   # encoder -> pty -> raw-input child, byte for byte
own-shell-spike omp-probe 12                # the installed omp in a pane: which modes it asks for, its first screen
powershell -File measure-idle.ps1 -Dir run-sys          # row 8, Windows
powershell -File fork-flood.ps1 -Mb 50 -Runs 3          # row 7, the fork
bash measure-idle.sh                                    # row 8, WSL
```

## 2. Rows 7 and 8, measured

### Row 7: 50 MiB (52,433,920 bytes of 80-column coloured lines) through one pane

Method. A stub (the spike's own exe, `stub-flood`) writes the bytes and then `FLOOD-END`, then keeps printing a dot every 100 ms so the pane stays alive. Time is from the pane's creation to the moment `FLOOD-END` is visible in the pane's grid.
- Spike: the main loop draws into a ratatui terminal over `io::sink()` (all escape sequences of a real frame are formatted; only the console write is left out), at most every 8 ms; the check reads the grid after each frame. The window is the 120x39 pane.
- Fork: a sandboxed `jocasta server` as `bench/footprint.rs` starts it (own data folder and herdr session, the bench binary as `omp` so the boss pane starts nothing real), one workspace whose pane runs the same stub through `layout.apply`; the time is polled with `pane.read` (`visible`) over herdr's socket every 20 ms, so it carries up to about 30 ms of polling. The fork's headless pane size is whatever herdr gives a pane with no client. The `stub's write done` time (the stub's own clock, which is bound by how fast the terminal drains the pty) gives the same picture.
- Driving the fork headless works through `bench`'s approach (herdr's pipe, one request per connection); it needed no keyboard.

| Run | Wall time, 50 MiB | Throughput |
|---|---|---|
| Spike, Windows, system ConPTY, 8 runs | 2.36, 2.39, 2.39, 2.41, 2.43, 2.51, 2.62, 2.69 s; median 2.42 s | 21.7 MB/s |
| Fork, Windows, `target/release/jocasta.exe` (no `conpty/` beside it), 3 runs | 5.44, 5.73, 6.62 s; median 5.73 s | 9.2 MB/s |
| Fork, Windows, same exe with the package's `conpty/` beside it, 3 runs | 5.21, 5.62, 6.80 s; median 5.62 s | 9.3 MB/s |
| Spike, Windows, bundled `conpty/` runtime | hangs: 0 of 4 runs of 50 MiB finished in 30 s; 1 MiB takes 5 s (section 3) | n/a |
| Spike, WSL Ubuntu, 3 runs | 0.645, 0.643, 0.640 s | 81 MB/s |

On Windows with the system ConPTY the spike is about 2.4 times the fork's throughput (pass: at least half). The fork on WSL wasn't run (it would need the Zig build there); the spike's number stands alone.

One 50 MiB run on the spike's system-ConPTY variant hung in the first attempt (the 250 s timeout killed it); it was not reproduced in the 8 runs after it, nor in runs of 1, 2, 5, 10, 20 and 30 MiB. Cause unknown (section 3, risk 1, may be related).

### Row 8: idle with 4 panes for 60 s

Method, as `bench/footprint.rs`: start the program, let it settle 10 s, then sum the working set of its process tree and the tree's CPU seconds over 60 s as a percent of one core. The panes' stub children (stand-ins for omp) are listed apart and not counted, as footprint counts the agents apart. The tree is the spike plus the ConPTY host process of each pane plus the console host of the spike's own (hidden) window.

| | Processes | RSS | CPU |
|---|---|---|---|
| Phase-5 budget (`bench/footprint-budget.json`: Jocasta's whole tree, herdr included, boss + 4 project panes) | 7 | 82.57 MB | 1.85% |
| Spike, Windows, system ConPTY (4 panes) | 6 (spike 24.9 MB + 5 `conhost` 8.4–10.3 MB) | 68.80 MB | 0.00% |
| Spike, Windows, bundled `conpty/` (4 panes) | 6 (spike 13.4 MB + 4 `OpenConsole` 11.0 MB + 1 `conhost` 10.3 MB) | 67.67 MB | 0.00% |
| Spike, WSL Ubuntu (4 panes) | 1 (+ 4 stub children, 9.0 MB, not counted) | 8.12 MB | 0.00% |

Pass: under the budget in all three. The budget counts five panes (the boss and four projects); the spike has four. Adding a fifth costs about one more host (about 9–11 MB on Windows), which still leaves it under 82.57 MB. The CPU figure is the resolution of the OS counter (about 15 ms in 60 s); idle is 0 by it. The spike's RSS grows when the grids fill (a full 10,000-line scrollback of 120 columns at roughly 24 bytes a cell is tens of MB per pane; not measured); the budget run's panes were empty, as the spike's.

Two honest limits: the fork's numbers are the committed budget, not a fresh run beside the spike's; and an omp in a pane doesn't change these numbers (omp is outside the budget in both).

## 3. What failed or looks risky

1. **The bundled ConPTY stalls under `alacritty_terminal`'s Windows tty (blocks row 7 there).** With `conpty.dll` + `x64\OpenConsole.exe` (1.24, as in the fork's package) beside the exe, a child that writes more than about 64 KB in a burst has its output held until the child exits (the `probe` encoder tests pass, so input is fine; `ping` and `cmd /c type` work; a 5-second-lived flood shows its output at second 5). Below the emulator too: reading the pty with no `Term` and no event loop shows the same 3–5 s hold. Not fixed by: ConPTY flags 0, 2, 4, 6, 7, 8 (set through the patched crate), or the output pipe size (64 KiB, 1 MiB). The fork on the same `conpty/` runs the same stub at 9.3 MB/s without stall, so it is something in `alacritty_terminal`'s pty layer (its pipes and `piper` reader, or flags it can't set: the fork's portable-pty passes `INHERIT_CURSOR | RESIZE_QUIRK | WIN32_INPUT_MODE`, and `alacritty_terminal` hard-codes 0). Options for the build: (a) ship no `conpty/` runtime and use the system ConPTY (works here, Windows 11; the fork bundles `conpty/` deliberately, see `crates/herdr/packaging/windows/conpty.json`; what the bundle buys was not examined here), (b) implement `EventedPty` + `OnResize` over the fork's vendored portable-pty (`EventLoop` is generic over them) so the shell keeps the fork's ConPTY handling, or (c) find the cause. (b) is real work in a build that was meant to delete the fork's code.
2. **One unexplained hang** in the very first 50 MiB run on the system ConPTY (see row 7). If it is a lost wake-up between `piper`/`polling` and the event loop, it would be a deadlock risk with any pane, so it needs a soak test before a build.
3. **No kitty negotiation and no bracketed paste from crossterm on Windows.** crossterm reads the console API there: `supports_keyboard_enhancement()` is always false, and `Event::Paste` never arrives. Press and release events do arrive (so F9 release can be forwarded when the pane asks for event types), and Shift/Ctrl/Alt come from the control-key state. A paste arrives as one key press and release per character, newlines as Enter. The spike's burst heuristic (8 or more text presses with gaps of 1 ms or less) turns that into one bracketed paste; it is untested with a real paste (row 3). A heuristic that guesses wrong would submit a half-pasted prompt. The build's alternative: read stdin in VT mode itself (`ENABLE_VIRTUAL_TERMINAL_INPUT`) and parse the host terminal's sequences, which also gives real bracketed paste and the host's kitty keys.
4. **omp's own terminal profile (measured with `omp-probe`, omp 19.1.1, 12 s after start, no prompt sent): it asks for kitty flag 1 (disambiguate) only**, bracketed paste and focus events; no flags for event types, no mouse mode, no alternate screen. Consequences: Shift+Enter and Ctrl+Enter reach omp as CSI-u (the encoder sends `ESC[13;2u`, `ESC[13;5u`, and the probe shows the child reads them unchanged); F9 release is **not** reported unless omp raises the flags later (when dictation is armed, say), which is exactly what row 2 must show; the mouse wheel and clicks aren't taken by omp at idle, so they act on the shell's own scrollback and selection.
5. **ConPTY passes the sequences it matters for**: `probe` through both the system and the bundled ConPTY, and on WSL, sent 22 cases (keys in legacy, `kitty` (flags 11) and `kitty1` (flags 1, omp's), SGR mouse, a 10,252-byte bracketed paste) through the encoder and the pty into a raw-input child: every one arrived byte for byte (0 of 22 different in each of the runs). The child's kitty push and query were honoured (`Term` took the flags and `alacritty_terminal` answered `ESC[?11u` / `ESC[?1u`). This proves the transport, not crossterm's reading of Windows Terminal (row 1 and 2 by hand).
6. **Query answers are canned.** OSC 10/11 get fixed colours (light on dark). omp may pick its theme from them; the build must ask the host terminal at start and answer with its colours.
7. **Not drawn or forwarded yet** (tabs-only scope, but listed so nothing is a surprise): underline colour, curly underlines, hyperlinks (OSC 8), cursor shape, DECSCUSR, synchronized output (2026) is honoured by the parser but the draw doesn't wait for it, the title is shown raw (`C:\…\omp.exe` appeared for a moment as a tab title), resize isn't throttled, and `ClipboardLoad` (OSC 52 read) answers empty.
8. **`windows-sys` 0.59 duplicate** (see section 1): `cargo deny` needs the `skip`, as 0683 said. `polling` 3.11 and `piper` are also new to the lock.
9. The grid-to-ratatui copy runs on every frame the active tab changed. At 21.7 MB/s the 50 MiB run took 150–170 frames and was not frame-bound; a very busy pane while a second one streams wasn't tried.

## 4. Rows 1–6: his procedure (one sitting, about 20 minutes)

Do it in Windows Terminal on the real keyboard, with `run-sys\own-shell-spike.exe` (add `-- omp --model @smol` where a cheap model is wanted). Compare against the same action in omp started directly in a second Windows Terminal tab. Write the verdict next to each row. Rows 1, 3, 4, 5 and 6 can be repeated on WSL with `wsl -d Ubuntu -e ~/own-shell-spike/target/release/own-shell-spike -- bash` (no omp there: use `bash` and `cat -v` where the row says so).

Before you start: `cd D:\jocasta\target\spikes\own-shell`. Start with `run-sys\own-shell-spike.exe`; F1 and F2 should show two omp tabs; type something in each and switch back: each tab keeps its own screen.

| # | Do | Pass | Verdict |
|---|---|---|---|
| 1 | In an omp tab, in the empty editor press each key and compare what omp does with the direct tab: Shift+Enter (new line in the editor), type `abc def`, then Alt+b (word back), Ctrl+Left, Ctrl+Right, Home, End, Left, Right, Up, Down, Backspace, Esc, Ctrl+C (clears the editor; twice at idle may quit omp: use the direct tab as reference). For Enter: type `/help` (a local command; no model call) and press Enter. Never press Enter on plain text (it would send a prompt). | each key acts exactly as in the direct tab | |
| 2 | In an omp tab, start dictation as you do directly (F9). Hold F9 for 3 s, then release. Then `own-shell-spike omp-probe 12` once more with omp's dictation enabled in its config, if that is what makes omp ask for more flags (`REPORT_EVENT_TYPES`). | recording starts on press and stops on release, as in the direct tab | |
| 3 | In PowerShell: `1..250 \| % { "paste line $_ with some text" } \| Set-Clipboard`. Paste with Ctrl+V into omp's editor in a tab (and then Ctrl+C to clear). Then, for exactness, in a second spike run `own-shell-spike.exe -- cmd /c "findstr /n ^ > D:\jocasta\target\spikes\paste.out"`, paste, press Ctrl+Z then Enter, and compare `paste.out` with the clipboard text (`Get-Clipboard`) line for line. On WSL: `-- bash -c "cat > /tmp/paste.out"`, paste, Ctrl+D, `diff` it. | omp shows one paste, nothing sent, nothing submitted line by line; the file holds all 250 lines unaltered | |
| 4 | Fill a tab (omp's long answer, or run a stub: `own-shell-spike.exe -- own-shell-spike.exe stub-flood 2 x`). Scroll with the wheel; drag with the mouse over some lines and release; paste in Notepad. Click in omp's editor. Then run `own-shell-spike.exe -- <a mouse app, e.g. omp's ask form or `bash -c "less README.md"` on WSL>` and wheel in it. | wheel scrolls the scrollback (or the app); drag selects and the text arrives in Notepad (OSC 52); a click in a mouse-aware app reaches it; Shift+drag selects inside it | |
| 5 | In an omp tab ask for a long answer (`print the numbers 1 to 400, one per line`), and drag the window's edge in and out while it streams, then wait for the next prompt and press a key. | after the next frame there is no garbling: no stale lines, no half-drawn rows, the cursor is in the editor; compare with the direct tab, which wraps the same way | |
| 6 | In a tab: `echo` or `type` a file with wide characters, emoji and truecolour (PowerShell: `"`e[38;2;255;100;0mTRUECOLOR`e[0m 日本語 😀 e`u{301}"`). Then, in omp: have it ask you a question with the `ask` form (``use the ask tool to ask me which of three colours I like``) and answer it with the keyboard; then enter plan mode (as you do directly), let it show the plan review and approve or reject it. | wide characters take two cells and don't shift the line, emoji show, the orange text is orange; the ask form and the plan review draw correctly and work | |

Automated evidence already in hand for these rows (not a verdict):
- Row 1 and 2, transport: all 22 encoder cases arrive unchanged through ConPTY (system and bundled) and on WSL (`probe`), for omp's flag profile too. Shift+Enter is `ESC[13;2u`, Ctrl+C is `ESC[99;5u`, Alt+b is `ESC[98;3u`, Ctrl+Right is `ESC[1;5C`, F9 press `ESC[20~` and, when event types are on, F9 release `ESC[20;1:3~`. With the kitty flags off the same keys are `\r`, `\r`, `ETX`, `ESC b`, `ESC[1;5C`, and F9 has no release.
- Row 2: omp 19.1.1's startup flags are 1 only (risk 4).
- Row 3: a 10,252-byte bracketed paste with newlines and wide characters arrives in 3–4 reads, byte for byte, between the markers.
- Row 6: a pane's grid took truecolour (`Spec(10,20,30)`) and 4 wide cells for `日本語` plus the emoji (`probe`; the combining mark wasn't checked).

## 5. What this says for the decision

- Row 7 passes with a wide margin (2.4 times the fork on Windows' system ConPTY, 0.64 s on WSL) and row 8 passes (68.8 MB against 82.57 MB, 0% CPU), so nothing in the numbers argues against the own shell.
- The open point that can still stop it is on Windows and is not in the checklist: the bundled ConPTY stalls under `alacritty_terminal`'s pty layer (risk 1). The build either runs on the system ConPTY or replaces that layer; decide before 0683's build step 1, and re-run `bench-flood 50` on the result.
- Rows 1–6 are his: the transport is proven, the keyboard is not.
- Record in 0683 when he has filled the verdicts. If rows 1–6 pass, phase 6 goes ahead with the choice in risk 1 made first; if any fails without a fix, the fork stays.
