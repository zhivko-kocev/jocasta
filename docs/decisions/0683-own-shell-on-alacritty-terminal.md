# 0683 Own shell on `alacritty_terminal`, gated by a spike

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Amends: 0649, 0659–0666
- Date: 2026-11-24

## Question
Can Jocasta replace the trimmed herdr fork with its own shell, and when?

## Facts
Measured 2026-11-24:
- `crates/herdr/src` + `crates/herdr/crates`: 248,846 + 12,331 lines (plus Zig, ghostty-vt, vendored portable-pty).
- herdr at runtime (state-of-the-art §6): 16 MB with no panes, 27.6 MB with 16 shells, 0.16% of a core idle.
- One working omp: 405 MB working set, 947 MB private. Outside Jocasta's panes: `__omp_worker_mnemopi_embed` 1,033 MB, `__omp_worker_tiny_inference` 641 MB.
- `jocasta --version` starts in 70.4 ms median (the `pane-check` role pays this at every turn end). A named-pipe JSON round trip takes 39 µs median, 53 µs p95.
- `target/release/jocasta.exe`: 47,279,616 bytes. Whiteboard's `node.exe … desktop-host.js` (41 MB) outlived Jocasta's Stop by hours. One PR-review thread cost $70.25.
- `alacritty_terminal` 0.26.0 (2026-04-06) passes the 7-day rule; it may bring `windows-sys` 0.59 beside 0.61.2.
- Losses accepted with the fork's removal: plugins, kitty graphics, live handoff, hand-made splits, copy mode, the settings overlay, restored screen history. Closing the terminal window ends Jocasta and its panes; threads come back through `--restore` at the next start (0549).

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep the fork | Works today; no risk | 248,846 + 12,331 lines, Zig, a second process and runtime weight |
| Own shell on `alacritty_terminal` in the same process, tabs only, after a spike | One process; code and memory fall; a surface of Jocasta's own | Risk on keys, paste, resize, throughput; work |
| Own terminal emulator | Full control | Far too much work |

## Recommendation
Own shell, but only if the spike passes.

## Choice
His answer of 2026-11-24. This record is the install approval for `alacritty_terminal` 0.26.0 (`=0.26.0`); a `skip` for `windows-sys@0.59` in `deny.toml` cites this record if `cargo deny` reports the duplicate.
- Spike gate: a throwaway package at `target/spikes/own-shell/` (outside the workspace) with one window, two tabs each running omp 19.1.1, results in `docs/research/own-shell-spike.md`, on Windows and WSL Ubuntu, with 8 checks: (1) Enter, Shift+Enter, Ctrl+C, Esc, arrows, Home/End, Alt+letter, Ctrl+arrows as in Windows Terminal; (2) kitty keyboard mode honoured, F9 press and release reach omp; (3) bracketed paste of 10 kB arrives once, unaltered; (4) mouse wheel and click in omp, selection and copy in the shell; (5) resize while omp streams leaves no garbling after the next frame; (6) wide characters, emoji, truecolor, omp's ask form and plan review; (7) 50 MB of output at least half the fork's throughput; (8) idle with 4 panes for 60 s, CPU and RSS no higher than the fork's phase-5 numbers.
- The phase 6 build happens only if all 8 rows pass (rows 1–6 failing without a fix inside the spike, or 7–8 failing, stops it). Otherwise the fork stays, phases 0–5 are the program, herdr's hook, `herdr.toml` and Zig stay, and the result is appended here.
- Build on pass: rename `Herdr` to `Surface`; `src/frontier/shell/` (`pty.rs`, `term.rs`, `keys.rs`, `app.rs`, `tabs.rs`, `sidebar.rs`; scrollback 10,000 lines; tabs only; draw only on damage); `own_surface.rs` implements `Surface` in-process, one `jocasta` process; agent status in the sidebar from `bridge.agent`; herdr's omp hook and `herdr.toml` removed by a migration; the bench ported to the pipe's `cli` messages with a new `pane_text {pane}`; `crates/herdr/`, `portable-pty`, `zig` and `config/herdr.toml` deleted; `mise run footprint` must show `processes` 1 and no number over the phase-5 budget; the new budget is committed.

## Reason
Lighter surface, if it proves itself.

## Consequences
- Amends 0649 (one binary, one server) and 0659–0666 (sidebar, ConPTY runtime, strip and workspace choices) on pass.
- Nothing in phases 0–5 depends on phase 6.

## Spike result (2026-11-24)
`docs/research/own-shell-spike.md`. Rows 7 and 8 pass on Windows and WSL Ubuntu: 50 MiB through one pane at 21.7 MB/s against the fork's 9.2 MB/s (WSL 81 MB/s); 4 idle panes for 60 s at 68.8 MB and 0.00% CPU against the phase-5 budget of 82.57 MB and 1.85%. Rows 1–6 wait for his keyboard. One blocker: with the bundled ConPTY runtime (`conpty/`, 0661), `alacritty_terminal`'s own pty holds a child's output past about 64 KB per burst until the child exits; the fork on the same runtime does not stall.

His answer of 2026-11-24: the own shell creates its terminals through the vendored portable-pty (the fork's, which works with the bundled runtime) under `alacritty_terminal`'s `Term`, so 0661's runtime stays. On pass, the build keeps `portable-pty` and its `[patch.crates-io]` entry (the build's deletion list loses them); `crates/herdr/`, Zig and `config/herdr.toml` still go. Rows 1–6 he runs later; the phase 6 build waits for them.

Spike re-run on the layer he chose (2026-11-24): the vendored portable-pty with the fork's flags (`INHERIT_CURSOR | RESIZE_QUIRK | WIN32_INPUT_MODE`) and the bundled `conpty/` beside the exe, feeding `alacritty_terminal`'s `Term`. No stall: 50 MiB finished in all 8 runs. Row 7: median 0.593 s, 88.5 MB/s against the fork's 9.2 MB/s (the fork's figure includes herdr's 20 ms `pane.read` poll); WSL 0.63 s. Row 8: 4 idle panes for 60 s at 63.61 MB and 0.00% CPU (WSL 4.12 MB). The 22 key, mouse and paste encodings arrive byte for byte. Rows 1–6 run on `target\spikes\own-shell\run\own-shell-spike.exe`; on Windows a paste still arrives as single key presses that the spike joins by timing, so row 3 decides that path.

Rows 1–6 (2026-11-24). His first pass by hand found typing lag during omp's suggestions, a stuttering cursor and no colour or emoji. By his word ("test everything, I am not needed") the agent ran the rows itself: a driver that reads the spike's screen as cells, then a live harness in a real Windows Terminal window with real key input and screenshots (`docs/research/own-shell-spike.md` sections 4a and 4b). It found and fixed four spike faults: the frame floor waiting in 15.6 ms timer steps, crossterm dropping colour under `NO_COLOR`, crossterm's Windows reader dropping typed and pasted emoji (the spike reads console input itself), and one keystroke drawn twice (frames coalesced, one write inside DEC 2026 synchronized output, the cursor set once). Verdicts: 1 keys pass (each key as in plain omp; key to screen p50 4.8–7.7 ms against 3.9 ms, one visible change per key as in plain omp); 2 F9 pass on delivery (press and release reach omp, which answers as it does in plain omp; his omp has speech-to-text off, so recording itself is unchecked); 3 paste pass (250 lines arrive once, line for line); 4 mouse pass (wheel, drag-select to the clipboard, SGR mouse in a mouse app); 5 resize pass (with a stub stream and omp's own screen; not during a streamed model answer); 6 drawing pass (truecolour, 日本語, 😀, omp's ask form and plan review). The gate passes, and the phase 6 build starts; the spike's four fixes carry into `src/frontier/shell/`.

## Built (2026-11-24)
- Terminals: each pane's terminal is made through the vendored `crates/portable-pty` (the fork's patched copy, outside the workspace, used by `[patch.crates-io]`) with the fork's ConPTY flags and the bundled `conpty/` runtime (0661), feeding `alacritty_terminal`'s `Term` through a `vte` `Processor`; one process holds the shell and the service (`run_embedded()`), and closing the window ends it. `Surface` is the seam (`FakeSurface` for tests, `OwnSurface` in the product).
- Keys: prefix Ctrl+B, then Alt+T (the `today` dock), Alt+F, Alt+S, Alt+R, Alt+N, Alt+Q (finish, stop, review, new thread, quit), G (goto an agent by name); Ctrl+B Ctrl+B sends a literal Ctrl+B; without the prefix Alt+G, Alt+1…9 and Alt+]/Alt+[ pick and cycle agents.
- Sidebar and chrome (the old `herdr.toml`'s defaults, 0401–0408, 0437, 0554): window title `Jocasta · <workspace>`, the tokyo-night theme, agents sorted blocked first with distinct state glyphs and two rows each (icon, workspace, `$model`, `$cost`; state text), the HUD (blocked count, today's spend, HH:MM) on the bottom status row, system toasts with sound for the ones that need him. Dropped: pane history across restarts and `resume_agents_on_restore`.
- Service: `cli` gained `pane_text {pane}`, `diagnose tab {command, cwd}` and `panes`; `jocasta diagnose` opens its tab through the service when Jocasta runs. Gone: herdr's words, `JOCASTA_SERVICE`, `JOCASTA_HERDR_SESSION`, the handoff. Setup's migration 5 is retired and migration 22 `herdr-gone` removes `<omp agent>/extensions/herdr-omp-agent-state.ts` and `<config>/herdr.toml`.
- Deleted: `crates/herdr/`, Zig, `config/herdr.toml`, `tests/herdr_config.rs`; `packaging/windows/` (the ConPTY pin) moved out of the fork. `deny.toml` lost the bincode and zvariant ignores, the 0BSD allowance and the wildcard-paths allowance; `cargo deny check` passes with no `windows-sys` skip needed.
- Closing the window (finish, 2026-11-24): a closed Windows pseudo console sends the child no input EOF, only `CTRL_CLOSE_EVENT`, and nothing handled it, so bare `jocasta` outlived its window. `shell/winin.rs::on_close` now installs `SetConsoleCtrlHandler`; on close, logoff or shutdown it asks the event loop to quit and holds the handler up to 30 s so the process ends by itself. `tests/entry.rs` closes by dropping the pty master and checks the service file, the client and every pane root pid are gone (5.5 s). Live, in a Windows Terminal window closed by handle: the whole tree (`jocasta`, its `conhost`, the boss's `omp`) was gone, by PID.
- `/api/shutdown` is gone with its door routes (`door_routes.rs`), `ask_shutdown` and the `reattach` path of the close guard: it had no client but the benches, which now close the window. 0675's line that `/api/shutdown` and `/health` stay on HTTP is amended: `/health` stays.
- `mise run smoke` exists again: one `live_smoke_…` test (`JOCASTA_HERDR_SMOKE=1`) starts bare jocasta in a pty with a fake omp, starts a thread through the CLI, answers its Allow in the terminal, reads the pane through `pane_text`, and closes the window. It dropped the "stop drops the thread" step (the fake omp doesn't speak the bridge) and the handoff step (gone with herdr).
- Live smoke (isolated data and config, release build, real omp 19.1.1): the sidebar, the boss's omp, typing, Alt+]/Alt+1/Alt+2/Alt+[ and a pasted emoji and Japanese line all rendered and worked; screenshots `target/spikes/own-shell/shots/phase6-01…07`. The status row reads `Ctrl+B Alt+T today · Alt+1-9 agents · Alt+G goto` with no dropped character.
- Footprint (release, three runs): `processes` 14, `idle_rss_mb` 167.9, `idle_cpu_pct` 2.5–2.9, `startup_ms` 60–92, `binary_mb` 21.76 (budget 39.44), `bridge_ms` 0.03–0.05. Over budget: `processes` (7 → 14) and `idle_rss_mb` (82.57). The budget was not rewritten. The own tree is the service (31 MB), 4 `jocasta pane-sleep` sleepers (12–16 MB each) and 9 ConPTY `conhost.exe` (8.4 MB each, one per pane). Reaching `processes` 1 needs the sleeping tabs drawn in the shell instead of a `pane-sleep` process, and an OS ConPTY host per omp pane cannot go; that is a change to 0676 and his decision.
- Sleeping panes and the count (2026-11-25, 0689): a sleeping pane is drawn by the shell with no process (`Pane::still`, asked for with `["jocasta:asleep", <text>]`; the sidebar labels it `asleep`), the `pane-sleep` role is gone, and the footprint leaves out the agents and the ConPTY hosts. Footprint (release, three runs): `processes` 1, `idle_rss_mb` 29.7–30.0, `idle_cpu_pct` 1.9–2.3 (one full refresh a minute: git and the thread facts), `startup_ms` 39–99, `binary_mb` 21.76, `bridge_ms` 0.04; the budget is written again from that count (ratchet: `processes` 1, `idle_rss_mb` 29.79, `binary_mb` 21.76). Live, in a Windows Terminal window: a thread start opened the project's pane asleep with no process behind it; one key started its omp; the window closed with nothing left running.
