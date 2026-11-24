# Own shell on `alacritty_terminal`: the spike (record 0683, 2026-11-24)

The spike of phase 6 of the lightweight program: can Jocasta's own shell, `alacritty_terminal` 0.26.0's `Term` over the fork's vendored portable-pty (his choice, after the first version of this spike used alacritty's own tty), drawn with ratatui and crossterm, replace the herdr fork? This file holds what was built, how to run it, rows 7 and 8 of 0683's checklist measured on Windows and on WSL Ubuntu, rows 1–6 as his manual procedure (verdicts empty), and what already failed or looks risky.

Verdict so far (updated: see section 4a, rows 1–6 run by a driver, two bugs found and fixed): **rows 7 and 8 pass on the shipped combination** (vendored portable-pty with the fork's flags, bundled `conpty/` beside the exe, `Term` fed by the spike's own reader): 50 MiB in 0.593 s median (about 9.7 times the fork) and 63.6 MB idle for 4 panes at 0% CPU against the 82.57 MB budget; the >64 KB stall that alacritty's own tty had under the bundled ConPTY is gone. WSL passes too. Rows 1–6 pass in the driver; the parts that need his hands and eyes are listed at the end of section 4a. Nothing in the repo changed except this file; the spike is untracked under `target/spikes/own-shell/`. The first version's numbers (alacritty's own tty, not shipped) are kept in section 2 for comparison.

## 1. What was built

A throwaway Cargo package, `target/spikes/own-shell/`, outside the workspace (an empty `[workspace]` table in its own `Cargo.toml`). About 1,400 lines in `src/`:

| File | What |
|---|---|
| `pane.rs` | one pane: a pty from the fork's vendored portable-pty (`openpty`, `spawn_command`, resize and writes through it) + `Term` (kitty keyboard on, 10,000 lines of scrollback) + a reader thread of its own (256 KiB reads → `vte`'s `Processor` → `Term`, one wake-up per batch) + a thread waiting for the child; the listener answers the pane's own queries (`PtyWrite`, OSC 10/11 colour requests with fixed colours, text-area size) and sends `Wakeup`, title and exit to the UI thread. `alacritty_terminal`'s `tty` and `EventLoop` are no longer used |
| `keys.rs` | crossterm key events → bytes: legacy, kitty CSI-u by the pane's `TermMode` flags (disambiguate, event types, all keys as escape codes, associated text), SGR and legacy mouse, bracketed paste |
| `view.rs` | `Term` grid → ratatui cells (colours, attributes, wide characters, zero-width marks, selection, cursor), and the tab bar |
| `main.rs` | the window: two tabs, one pty each, raw mode, alternate screen, mouse capture, bracketed paste; a UI thread that draws only on damage (at most every 8 ms) and never draws the hidden tab; local selection and OSC 52 copy; wheel scrolls the shell's scrollback (or sends arrows in an alternate-screen app); on Windows a burst of 8 or more text key presses within 1 ms each is taken for a paste |
| `stubs.rs` | stand-ins run in panes: `stub-flood`, `stub-idle`, `stub-input` (raw-input recorder) |
| `probe.rs` | `probe` and `omp-probe`: automated parts of rows 1–3 and 6 |
| `forkdrive.rs`, `*.ps1`, `*.sh` | the measuring drivers |

Dependencies: `alacritty_terminal =0.26.0`, `portable-pty` by path (`crates/herdr/vendor/portable-pty`, the copy the root `Cargo.toml`'s `[patch.crates-io]` points at; the patch is read from a workspace root only, so the spike names the path itself), `ratatui =0.30.2`, `crossterm =0.29.0` (the repo lock's versions), `libc` on unix (in the repo lock). The spike's `Cargo.lock` started from the repo's and adds only what `alacritty_terminal` pulls: `arrayvec 0.7.8`, `cursor-icon 1.2.0`, `hermit-abi 0.5.3`, `home 0.5.12`, `miow 0.6.1`, `piper 0.2.5`, `polling 3.11.0`, `rustix-openpty 0.2.0`, `signal-hook 0.4.6`, `vte 0.15.0`, `windows-sys 0.59.0` (the duplicate 0683 predicted: it sits beside the repo's 0.52 and 0.61.2; `cargo deny` would need the `skip`). Nothing came from outside those. The repo's 7-day rule applied (`.cargo/config.toml` is found from `target/spikes/`).

The first version patched alacritty's crate to read ConPTY flags and the pipe size from env vars while hunting the stall; that patch and its `vendor/` copy are gone.

Two tabs, each running omp 19.1.1: yes. `own-shell-spike -- omp` starts two omp panes; `own-shell-spike -- omp --version` showed both start and print `omp/19.1.1` in the grid and then show `[exited]` in the tab. `omp-probe` (below) started a real omp and read its first screen from the grid.

### Run it

```
cd D:\jocasta\target\spikes\own-shell
cargo build --release
copy target\release\own-shell-spike.exe run\       # beside run\conpty\ (the bundled runtime)
run\own-shell-spike.exe                             # the one run command: two tabs, each `omp`
run\own-shell-spike.exe -- omp --model @smol        # any other command after --
```

Keys of the spike itself: `F1`, `F2` switch tabs (also a click on the tab bar), `F12` quits. Everything else goes to the pane. Plain drag selects (Shift+drag when the program uses the mouse); releasing copies through OSC 52. `run\` holds the exe and `conpty\` (the bundled runtime: `conpty.dll`, `herdr-conpty.json`, `x64\` and `arm64\` `OpenConsole.exe`) copied from `dist\windows-x64\conpty\`, which `scripts/build.ts` lays out from `crates/herdr/packaging/windows/conpty.json`; the vendored portable-pty loads it only when every file and the marker are beside the exe, as in the Windows package. The folders `run-sys\` and `run-conpty\` belong to the old alacritty-tty variant and are not shipped. WSL: `wsl -d Ubuntu -e ~/own-shell-spike/target/release/own-shell-spike -- bash` (a copy built there with Rust 1.100.0, `--locked`, the portable-pty copy beside it; omp isn't installed in WSL, so use `bash`, `cat -v` or another command). A pane's working directory is portable-pty's default (the home folder), not the shell's.

The measuring commands:

```
own-shell-spike bench-flood 50              # row 7: 50 MiB through one pane, 120x39 grid, frames formatted into a sink
own-shell-spike bench-idle 4 60             # row 8: 4 idle panes (measured from outside)
own-shell-spike probe kitty|kitty1|legacy   # encoder -> pty -> raw-input child, byte for byte
own-shell-spike omp-probe 12                # the installed omp in a pane: which modes it asks for, its first screen
powershell -File measure-idle.ps1 -Dir run          # row 8, Windows
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
| **Spike, Windows, portable-pty layer + bundled `conpty/` (`run\`; the shipped path), 8 runs** | 0.566, 0.578, 0.592, 0.592, 0.593, 0.594, 0.628, 0.673 s; **median 0.593 s** (the stub's own write-done clock: 0.54–0.65 s) | **88.5 MB/s** |
| **Spike, WSL Ubuntu, portable-pty layer, 3 runs** | 0.641, 0.632, 0.626 s | 83 MB/s |
| (alacritty's own tty, not shipped) Spike, Windows, system ConPTY, 8 runs | 2.36, 2.39, 2.39, 2.41, 2.43, 2.51, 2.62, 2.69 s; median 2.42 s | 21.7 MB/s |
| Fork, Windows, `target/release/jocasta.exe` (no `conpty/` beside it), 3 runs | 5.44, 5.73, 6.62 s; median 5.73 s | 9.2 MB/s |
| Fork, Windows, same exe with the package's `conpty/` beside it, 3 runs | 5.21, 5.62, 6.80 s; median 5.62 s | 9.3 MB/s |
| (alacritty's own tty, not shipped) Spike, Windows, bundled `conpty/` | hangs: 0 of 4 runs of 50 MiB finished in 30 s; 1 MiB takes 5 s (section 3) | n/a |
| (alacritty's own tty, not shipped) Spike, WSL Ubuntu, 3 runs | 0.645, 0.643, 0.640 s | 81 MB/s |

On the shipped path the spike takes 0.593 s against the fork's 5.62–5.73 s (median of 3 each, with and without `conpty/`): about 9.7 times the fork's throughput (pass: at least half). The old alacritty-tty variant on the system ConPTY took 2.42 s (2.4 times the fork). The fork on WSL wasn't run (it would need the Zig build there); the spike's number stands alone.

(Alacritty tty, not shipped.) One 50 MiB run on the spike's system-ConPTY variant hung in the first attempt (the 250 s timeout killed it); it was not reproduced in the 8 runs after it, nor in runs of 1, 2, 5, 10, 20 and 30 MiB. Cause unknown (section 3, risk 1, may be related).

### Row 8: idle with 4 panes for 60 s

Method, as `bench/footprint.rs`: start the program, let it settle 10 s, then sum the working set of its process tree and the tree's CPU seconds over 60 s as a percent of one core. The panes' stub children (stand-ins for omp) are listed apart and not counted, as footprint counts the agents apart. The tree is the spike plus the ConPTY host process of each pane plus the console host of the spike's own (hidden) window.

| | Processes | RSS | CPU |
|---|---|---|---|
| Phase-5 budget (`bench/footprint-budget.json`: Jocasta's whole tree, herdr included, boss + 4 project panes) | 7 | 82.57 MB | 1.85% |
| **Spike, Windows, portable-pty layer + bundled `conpty/` (`run\`; the shipped path), 4 panes** | 6 (spike 9.3 MB + 4 `OpenConsole` 11.0 MB + 1 `conhost` 10.3 MB) | **63.61 MB** | **0.00%** |
| **Spike, WSL Ubuntu, portable-pty layer, 4 panes** | 1 (+ 4 stub children, 9.5 MB, not counted) | **4.12 MB** | **0.00%** |
| (alacritty's own tty, not shipped) Spike, Windows, system ConPTY (4 panes) | 6 (spike 24.9 MB + 5 `conhost` 8.4–10.3 MB) | 68.80 MB | 0.00% |
| (alacritty's own tty, not shipped) Spike, Windows, bundled `conpty/` (4 panes) | 6 (spike 13.4 MB + 4 `OpenConsole` 11.0 MB + 1 `conhost` 10.3 MB) | 67.67 MB | 0.00% |
| (alacritty's own tty, not shipped) Spike, WSL Ubuntu (4 panes) | 1 (+ 4 stub children, 9.0 MB, not counted) | 8.12 MB | 0.00% |

Pass: the shipped path (63.61 MB) is under the budget (82.57 MB), as are the others. The budget counts five panes (the boss and four projects); the spike has four. Adding a fifth costs about one more host (about 9–11 MB on Windows), which still leaves it under 82.57 MB. The CPU figure is the resolution of the OS counter (about 15 ms in 60 s); idle is 0 by it. The spike's RSS grows when the grids fill (a full 10,000-line scrollback of 120 columns at roughly 24 bytes a cell is tens of MB per pane; not measured); the budget run's panes were empty, as the spike's.

Two honest limits: the fork's numbers are the committed budget, not a fresh run beside the spike's; and an omp in a pane doesn't change these numbers (omp is outside the budget in both).

## 3. What failed or looks risky

1. **Resolved by his choice: alacritty's own tty stalls under the bundled ConPTY; the vendored portable-pty does not.** The first version of the spike used `alacritty_terminal`'s `tty` and `EventLoop`. With the bundled `conpty.dll` + `x64\OpenConsole.exe` beside the exe, a child that writes more than about 64 KB in a burst had its output held until it exited (1 MiB took 5 s, 50 MiB never finished: 0 of 4 runs in 30 s); the same hold showed with no `Term` at all, and ConPTY flags 0, 2, 4, 6, 7, 8 and the output pipe size didn't change it. That tty hard-codes flags 0. The spike now creates its ptys with the fork's vendored portable-pty (flags `INHERIT_CURSOR | RESIZE_QUIRK | WIN32_INPUT_MODE`, the bundled `conpty\` loaded because every file and `herdr-conpty.json` sit beside the exe; `OpenConsole.exe` processes show in the idle tree), runs its own reader thread (256 KiB reads → `vte`'s `Processor` → `Term`), and writes input, resize and the queries' answers through portable-pty. The stall is gone (section 2). `alacritty_terminal`'s `tty`/`event_loop` modules are unused but still compiled, so its `polling`, `piper`, `miow` and `windows-sys` 0.59 still enter the lock.
2. **One unexplained hang** in the very first 50 MiB run on alacritty's own tty with the system ConPTY (not shipped; 1 of 13 runs, never again). With the portable-pty layer: 0 hangs in 8 runs of 50 MiB on Windows and 3 on WSL, and 4 probes. No soak test was done.
3. **No kitty negotiation and no bracketed paste from crossterm on Windows.** crossterm reads the console API there: `supports_keyboard_enhancement()` is always false, and `Event::Paste` never arrives. Press and release events do arrive (so F9 release can be forwarded when the pane asks for event types), and Shift/Ctrl/Alt come from the control-key state. A paste arrives as one key press and release per character, newlines as Enter. The spike's burst heuristic (8 or more text presses with gaps of 1 ms or less) turns that into one bracketed paste; it is untested with a real paste (row 3). A heuristic that guesses wrong would submit a half-pasted prompt. The build's alternative: read stdin in VT mode itself (`ENABLE_VIRTUAL_TERMINAL_INPUT`) and parse the host terminal's sequences, which also gives real bracketed paste and the host's kitty keys. **This is the same with the portable-pty layer: the input side is crossterm's, so the paste heuristic still applies on Windows (it is the only paste path there) and row 3 tests it.**
4. **omp's own terminal profile (measured with `omp-probe`, omp 19.1.1, 12 s after start, no prompt sent): it asks for kitty flag 1 (disambiguate) only**, bracketed paste and focus events; no flags for event types, no mouse mode, no alternate screen. Consequences: Shift+Enter and Ctrl+Enter reach omp as CSI-u (the encoder sends `ESC[13;2u`, `ESC[13;5u`, and the probe shows the child reads them unchanged); F9 release is **not** reported unless omp raises the flags later (when dictation is armed, say), which is exactly what row 2 must show; the mouse wheel and clicks aren't taken by omp at idle, so they act on the shell's own scrollback and selection.
5. **The pty layers pass the sequences it matters for**: `probe` through the portable-pty layer with the bundled ConPTY (the shipped path), through alacritty's own tty with either ConPTY, and on WSL, sent 22 cases (keys in legacy, `kitty` (flags 11) and `kitty1` (flags 1, omp's), SGR mouse, a 10,252-byte bracketed paste) through the encoder and the pty into a raw-input child: every one arrived byte for byte (0 of 22 different in each of the runs). The child's kitty push and query were honoured (`Term` took the flags and `alacritty_terminal` answered `ESC[?11u` / `ESC[?1u`). This proves the transport, not crossterm's reading of Windows Terminal (row 1 and 2 by hand).
6. **Query answers are canned.** OSC 10/11 get fixed colours (light on dark). omp may pick its theme from them; the build must ask the host terminal at start and answer with its colours.
7. **Not drawn or forwarded yet** (tabs-only scope, but listed so nothing is a surprise): underline colour, curly underlines, hyperlinks (OSC 8), cursor shape, DECSCUSR, synchronized output (2026) is honoured by the parser but the draw doesn't wait for it, the title is shown raw (`C:\…\omp.exe` appeared for a moment as a tab title), resize isn't throttled, and `ClipboardLoad` (OSC 52 read) answers empty.
8. **`windows-sys` 0.59 duplicate** (see section 1): `cargo deny` needs the `skip`, as 0683 said. `polling` 3.11 and `piper` are also new to the lock.
9. The grid-to-ratatui copy runs on every frame the active tab changed. At 88 MB/s the 50 MiB run took 36–42 frames and was not frame-bound; a very busy pane while a second one streams wasn't tried.

## 4. Rows 1–6: his procedure (one sitting, about 20 minutes)

Do it in Windows Terminal on the real keyboard, with `run\own-shell-spike.exe` (add `-- omp --model @smol` where a cheap model is wanted). Compare against the same action in omp started directly in a second Windows Terminal tab. Write the verdict next to each row. Rows 1, 3, 4, 5 and 6 can be repeated on WSL with `wsl -d Ubuntu -e ~/own-shell-spike/target/release/own-shell-spike -- bash (rebuilt on the portable-pty layer)` (no omp there: use `bash` and `cat -v` where the row says so).

Before you start: `cd D:\jocasta\target\spikes\own-shell`. Start with `run\own-shell-spike.exe`; F1 and F2 should show two omp tabs; type something in each and switch back: each tab keeps its own screen.

| # | Do | Pass | Verdict |
|---|---|---|---|
| 1 | In an omp tab, in the empty editor press each key and compare what omp does with the direct tab: Shift+Enter (new line in the editor), type `abc def`, then Alt+b (word back), Ctrl+Left, Ctrl+Right, Home, End, Left, Right, Up, Down, Backspace, Esc, Ctrl+C (clears the editor; twice at idle may quit omp: use the direct tab as reference). For Enter: type `/help` (a local command; no model call) and press Enter. Never press Enter on plain text (it would send a prompt). | each key acts exactly as in the direct tab | |
| 2 | In an omp tab, start dictation as you do directly (F9). Hold F9 for 3 s, then release. Then `own-shell-spike omp-probe 12` once more with omp's dictation enabled in its config, if that is what makes omp ask for more flags (`REPORT_EVENT_TYPES`). | recording starts on press and stops on release, as in the direct tab | |
| 3 | In PowerShell: `1..250 \| % { "paste line $_ with some text" } \| Set-Clipboard`. Paste with Ctrl+V into omp's editor in a tab (and then Ctrl+C to clear). Then, for exactness, in a second spike run `own-shell-spike.exe -- cmd /c "findstr /n ^ > D:\jocasta\target\spikes\paste.out"`, paste, press Ctrl+Z then Enter, and compare `paste.out` with the clipboard text (`Get-Clipboard`) line for line. On WSL: `-- bash -c "cat > /tmp/paste.out"`, paste, Ctrl+D, `diff` it. | omp shows one paste, nothing sent, nothing submitted line by line; the file holds all 250 lines unaltered | |
| 4 | Fill a tab (omp's long answer, or run a stub: `own-shell-spike.exe -- own-shell-spike.exe stub-flood 2 x`). Scroll with the wheel; drag with the mouse over some lines and release; paste in Notepad. Click in omp's editor. Then run `own-shell-spike.exe -- <a mouse app, e.g. omp's ask form or `bash -c "less README.md"` on WSL>` and wheel in it. | wheel scrolls the scrollback (or the app); drag selects and the text arrives in Notepad (OSC 52); a click in a mouse-aware app reaches it; Shift+drag selects inside it | |
| 5 | In an omp tab ask for a long answer (`print the numbers 1 to 400, one per line`), and drag the window's edge in and out while it streams, then wait for the next prompt and press a key. | after the next frame there is no garbling: no stale lines, no half-drawn rows, the cursor is in the editor; compare with the direct tab, which wraps the same way | |
| 6 | In a tab: `echo` or `type` a file with wide characters, emoji and truecolour (PowerShell: `"`e[38;2;255;100;0mTRUECOLOR`e[0m 日本語 😀 e`u{301}"`). Then, in omp: have it ask you a question with the `ask` form (``use the ask tool to ask me which of three colours I like``) and answer it with the keyboard; then enter plan mode (as you do directly), let it show the plan review and approve or reject it. | wide characters take two cells and don't shift the line, emoji show, the orange text is orange; the ask form and the plan review draw correctly and work | |

Automated evidence already in hand for these rows (not a verdict):
- Row 1 and 2, transport: all 22 encoder cases arrive unchanged through the portable-pty layer with the bundled ConPTY (and through alacritty's own tty with either ConPTY, and on WSL; `probe`), for omp's flag profile too. Shift+Enter is `ESC[13;2u`, Ctrl+C is `ESC[99;5u`, Alt+b is `ESC[98;3u`, Ctrl+Right is `ESC[1;5C`, F9 press `ESC[20~` and, when event types are on, F9 release `ESC[20;1:3~`. With the kitty flags off the same keys are `\r`, `\r`, `ETX`, `ESC b`, `ESC[1;5C`, and F9 has no release.
- Row 2: omp 19.1.1's startup flags are 1 only (risk 4).
- Row 3: a 10,252-byte bracketed paste with newlines and wide characters arrives in 3–4 reads, byte for byte, between the markers.
- Row 6: a pane's grid took truecolour (`Spec(10,20,30)`) and 4 wide cells for `日本語` plus the emoji (`probe`; the combining mark wasn't checked).

## 4a. The driver: rows 1–6 run without a keyboard (2026-11-24, after his first hand run)

His first hand run reported lag on Backspace and on typing a space while omp showed its suggestion list, occasional scroll stutter, and no truecolour or emoji. `own-shell-spike drive` reproduces and measures all of it.

**What it is.** `drive <ref|spike> <COLSxROWS> [--cmd <words…>] --script <tokens…>` starts a command inside a portable-pty session of its own (same layer, bundled `conpty/`), reads the screen that command draws through its own `alacritty_terminal::Term` (so every outer cell is available: char, fg/bg, wide flag), and sends input the way Windows Terminal does: win32-input-mode key events (`ESC [ vk ; sc ; uc ; down ; ctrl-state ; 1 _`, press and release, modifier keys around modified ones), SGR mouse, resize. `spike` runs `own-shell-spike -- <cmd>`; `ref` runs `<cmd>` directly (the same driver, no spike in between). Each parsed batch of output logs `(time, hash of all visible cells and cursor)`, so latency is "from the write of the key to the first batch that changes the screen", independent of any polling. `{exe}` in a word is the spike's path. Tokens: `t:<text>` (3 ms per key), `k:<Key>` (`Enter Backspace Esc Tab Left Right Up Down Home End PgUp PgDn Del F9`, with `C-` `A-` `S-` prefixes), `w:<ms>`, `q:<ms>` (wait for quiet), `dump:<name>`, `find:<text>` (cells of the first match: width, fg, bg), `lat:<series>:<n>:<gap ms>:<token>` (p50/p95), `stamp:<token>`, `m:<d|u|drag|wu|wd>:<x>:<y>`, `scroll:<n>:<gap ms>:<up|down>`, `frames:<ms>`, `paste:<lines>`, `resize:<c>x<r>`, `clips`, `mode`. The scripts used are beside the spike: `lat1.sh`, `row1.sh`, `row5.sh`. For the reference the outer size is one row shorter (the spike's tab bar takes row 0).

Two of the commands that bit me: omp's `Enter` is a real model call (an unwanted `Shift+Enter` on the reference submitted `abc def` to Opus once: the reference gets `\r` for Shift+Enter, see row 1), and typing a long string in one write looks like a paste to the spike's burst heuristic, so the driver types with 3 ms between keys.

### Found and fixed (both reproduced before the fix)

|#|Cause|Fix|Effect|
|---|---|---|---|
|1|**The lag.** `Receiver::recv_timeout` waits in steps of about 15.6 ms on Windows. The UI loop waited for the 8 ms frame floor with it, so a frame that arrived a few ms after a keystroke's first draw (omp redraws twice per key: the echo, then the suggestion list) was drawn 11–18 ms late; every key press also forced a redraw, which started that floor. Trace: input at +0.2 ms, omp's second batch at +4.5 ms, drawn at +10.9 or +17 ms.|The floor is waited with `thread::sleep` (high resolution in std on Windows) and is 4 ms; a key sent to the pane no longer marks the screen dirty (only mouse, resize, tab switches and a cleared selection do); the paste-burst check uses `poll(0)` instead of `poll(1 ms)` per key.|typing p50 19.2 → **5.9 ms**, p95 20.9 → **7.2 ms**; Backspace p50 19.3 → **5.9**, p95 20.8 → **6.7**. Plain omp in the same driver: 4.0 / 9.3 and 3.8 / 5.0 (three later runs: spike p95 6.9–7.8, reference 6.0–6.4).|
|2|**No colour** when the environment has `NO_COLOR`: crossterm honours it and turns every colour command into nothing. My shell has `NO_COLOR=1`, and so, apparently, did the shell his first run started from (on a clean environment the colours were already right). A terminal that hosts other programs must not apply it.|`crossterm::style::force_color_output(true)` at start.|orange fg and blue bg arrive as RGB with `NO_COLOR=1` set.|

### Results by row (Windows, `run\own-shell-spike.exe`; the same inputs on plain omp as the reference)

|Row|What was run|Result|
|---|---|---|
|1 keys|`abc def`, a key, `X`, read omp's editor: Home, End, Left, Ctrl+Left, Alt+b, Alt+Left, Backspace, Ctrl+W, Ctrl+A, Ctrl+E, Shift+Enter, Esc, Up, Ctrl+C|13 of 14 identical to the reference (`Xabc def`, `abc defX`, `abc deXf`, `abc Xdef` ×3, `abc deX`, `abc X`, `Xabc def` ×2, `abc defX` ×2, `X` after Ctrl+C). **Shift+Enter differs, in the spike's favour:** the reference (ConPTY turns win32-input Shift+Enter into a plain `\r`) *submitted* the prompt; the spike sent the kitty `ESC[13;2u` omp asked for and omp made a new line (`abc def` / `X`). Whether Windows Terminal gives plain omp the newline as well is his to check on the left tab. **Pass.**|
|1 latency|25 typed `e` and 25 Backspaces with the slash-command list open, 90 ms apart, no model call|before the fix spike p50/p95 19.2/20.9 and 19.3/20.8 ms vs reference 4.0/9.3 and 3.8/5.0; **after: spike 5.9/7.2 and 5.9/6.7 vs reference 4.0/9.3, 3.8/5.0** (second run 5.6/6.9, 5.5/7.8 vs 4.2/6.4, 4.1/6.0). Bar: reference + 16 ms at p95. **Pass** (+1.5 to +2 ms at p50). A single space and the Backspace after it: 5.7 and 5.3 ms.|
|2 F9|F9 press and release against a stub that asks for what omp asks for|`stub-input kitty` (flags 11, event types): `ESC[20~` then `ESC[20;1:3~` per tap, i.e. press and release both arrive. `kitty1` (flag 1, **what omp 19.1.1 actually asks for**) and legacy: `ESC[20~` only, no release (the protocol has none without event types). The spike sends the same as Windows Terminal would for the same flags. F9 was **not** sent to real omp (dictation may record audio): **his eyes: hold F9 in a tab, check recording starts and stops on release; if it does not stop, omp with flag 1 has no release event in any terminal.**|
|3 paste|250 numbered lines typed as key bursts (what Windows Terminal gives a console app) into a cooked-mode line reader writing a file, and into a child that asked for bracketed paste|file identical line for line to the expected 250 lines, for the spike and for the reference. With bracketed paste asked, the child received one `ESC[200~ … ESC[201~` with the 250 lines (7,409 bytes), unaltered. **Pass** (the heuristic made the bracketed paste from key events). Real Ctrl+V in Windows Terminal is his to try: it uses the same path.|
|4 mouse|wheel on 200 lines of scrollback; drag-select; SGR mouse to a mouse-mode app|30 notches scroll exactly 90 lines up and back; 60 notches at 4 ms: 54–55 changed frames, gaps p50 4.6, p95 4.9–5.2, **max 5.3–6.1 ms (no stutter)**. Drag from col 4 to col 20 on a row: the OSC 52 write carried `175 of the scroll`, the exact selected text. A mouse-mode app got `ESC[<0;10;4M`, `ESC[<32;13;4M`, `ESC[<0;13;4m`, `ESC[<64;6;5M`, `ESC[<65;6;5M` (press, drag, release, wheel up and down) with the tab bar's row removed. **Pass.** His eyes: that Windows Terminal then puts the OSC 52 text on the clipboard.|
|5 resize|four resizes (120×40 → 100×30 → 140×45 → 110×35 → 120×40) while 12,000 numbered lines stream at 2,000/s; omp idle with the command list open, resized three times|after the next frame the 38 body rows were consecutive and **identical to a fresh spike at the final size**, 4 runs of 4; omp's screen after the resizes **identical** to a fresh omp's. **Pass.** Not run: resize while omp streams a model answer.|
|6 colour, width|`powershell -NoProfile -Command "…38;2;255;100;0m ORANGE … 48;2;0;80;160m BLUEBG … 日本語 😀"`|outer cells: ORANGE fg `Rgb(255,100,0)`, BLUEBG bg `Rgb(0,80,160)`, 日 本 語 and 😀 each a wide cell plus its spacer (2 columns), the same as the reference. **Pass** with the fix above (before: no colour with `NO_COLOR=1`). The emoji not showing on his screen was not reproducible in the cells: the spike delivers them; a missing glyph is his terminal font or his typing the line into an omp tab (omp's editor does not run PowerShell). His eyes: open `own-shell-spike.exe -- powershell` and paste the line.|
|6 omp forms|real model with `--model @smol` (cents): the ask tool's form|drawn and usable: `❯` moves with Down, Enter on Beta ended the form, omp answered `The user picked Beta.`; the status row and tab labels updated (`ASK: …`). A 120-line streamed answer: ended with the same screen as the reference, 212 batches seen at the outer terminal (the spike writes a frame in several pieces, so batch gaps are not frame gaps), longest gap 440 ms, same as the reference's 437 ms (the model). **Not run: plan review** (needs a plan-mode turn); his eyes.|

### Still needing his eyes

Real-keyboard feel of typing (the driver sends win32 events, not a physical keyboard, and measures to the outer screen buffer, not to pixels), F9 dictation actually recording, Ctrl+V and OSC 52 through Windows Terminal itself, plan review, emoji glyphs in his font, and Shift+Enter in plain omp versus the spike. Rows 7 and 8 stay as measured in section 2; the frame-floor change did not move row 7 (0.59 → 0.60, 0.63, 0.76 s in three runs).

## 5. What this says for the decision

- With the pty layer he chose (vendored portable-pty, the fork's flags, the bundled `conpty/` beside the exe), row 7 passes by a wide margin (0.59 s for 50 MiB, about 9.7 times the fork's throughput on Windows; 0.63 s on WSL) and row 8 passes (63.6 MB against 82.57 MB, 0% CPU). The >64 KB stall of alacritty's own tty does not occur.
- Nothing in the numbers argues against the own shell. Two caveats: the fork's numbers were measured through herdr's headless socket and a 20 ms `pane.read` poll (Windows), so the ratio is an estimate of the whole stack, not of the pty layer alone; and the 8 runs and one 4-pane idle minute are a small sample (no soak test; the one unexplained hang of the old tty was never seen again here: 8 of 8 plus 3 of 3 on WSL).
- Rows 1–6 ran through the driver (section 4a): they pass after two fixes (the 15 ms wait that made typing 19 ms behind, and `NO_COLOR`); his eyes are still needed for the items listed there. The Windows paste heuristic (risk 3) still applies and is what row 3 tests.
- Record in 0683 when he has filled the verdicts. If rows 1–6 pass, phase 6 goes ahead; if any fails without a fix, the fork stays.
