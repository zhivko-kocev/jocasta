<!-- The prewalk's plan: Fable 5.2 at xhigh, read-only, 2026-11-18, from docs/research/two-plugins.md; the build follows it, and docs/research/fork-build.md is the ledger of what was built and chosen. -->

# Plan: Jocasta as a herdr fork plus an omp snapshot (prewalk, 2026-11-18)

Read-only review against `D:/jocasta-spikes/herdr-0.9.5` and `D:/jocasta`. Nothing was built or run. `[INFERENCE]` marks what I did not read in source. Line numbers marked `~` are approximate.

## 0. Rules for the whole run

- **One heavy job at a time.** Cargo, the Zig build, a release build and an eval trial never overlap. One shared `target/`, no parallel compiling subagents.
- **Language server off.** Start the build session without rust-analyzer; herdr is about 300k lines, and analyzer plus cargo plus link will not fit in 16 GB [INFERENCE]. Use `cargo check -p <crate>` instead.
- **Build parallelism.** `CARGO_BUILD_JOBS=4` in `mise.toml` `[env]`, 3 on the macOS runner, 2 for release builds. Before any cargo job require 5 GB available, else wait.
- **Debug info.** `[profile.dev]` and `[profile.test]`: `debug = "line-tables-only"`, and `debug = false` for `[profile.dev.package."*"]`. This cuts link memory and PDB size.
- **Release profile.** `Cargo.toml` has `lto = true, codegen-units = 1`. Measure the peak memory of one release build with herdr linked in; above 8 GB switch to `lto = "thin"`, `codegen-units = 16`. Upstream herdr ships without LTO.
- **Zig.** `crates/herdr/crates/ghostty-vt/build.rs:74-84` calls `zig build` with no job limit. Add `-j2` (a two-line fork patch). It runs once and caches in `OUT_DIR`.
- **Tests.** Add `.config/nextest.toml`: default `test-threads = 4`, a test group with `max-threads = 2` for herdr's binary-spawning integration tests and Jocasta's `e2e_*`. Inner loop is `cargo nextest run -p jocasta -E 'test(…)'`; the herdr suite runs only at fork-touching gates and in CI.
- **Disk.** `status.md` records C: at 8% free. Keep `target/` on D:, check free space before phase 1, and `cargo clean -p herdr` when under 30 GB.
- **Paperwork.** By his word the records wait. Keep a ledger in `docs/research/fork-build.md` of every visible choice as built (0363), and write records 0649+ from it and from `two-plugins.md` in the last phase.
- **Commits.** One per phase after its gate passes, under the repo identity. Do not push in this run (see section 5, item 1).
- **Fork hygiene.** Delete whole files; never reformat or restyle kept herdr files. Every edit inside a kept upstream file is a seam, listed in `crates/herdr/FORK.md`. Target at most 15 seam files.

## 1. Gaps, contradictions and risks, with the choice

| # | Fact, checked in source | Choice |
|---|---|---|
| 1 | herdr is a bin-only crate: `src/main.rs:8-58` is the `mod` list, there is no `lib.rs`. | Add `src/lib.rs` holding the `mod` list and `pub fn main_with(embed)`. Keep a thin `[[bin]] herdr` for herdr's own integration tests; it is not shipped. |
| 2 | `run_server` (`server/headless/bootstrap.rs:4`) reads `std::env::args()` by position (`:6-7`, `:19-25`), calls `process::exit(1)` on a busy socket (`:49`, `:82`), builds its own tokio runtime (`:54`) and has no hook point. | Split into `run_server_with(options)`: handoff args passed in, `Err` instead of exit, and an extension started beside `run_plugin_startup_hooks` (`:95`, and in the handoff import path). The inference in `two-plugins.md` holds. |
| 3 | Process-wide state: about 30 statics (`session.rs:18`, `layout.rs:14`, `workspace.rs:104`, `terminal/id.rs:13`, `detect/manifest.rs:265`, `platform/windows.rs:24,702-961`), a global tracing subscriber (`logging.rs:12-31`), `ctrlc::set_handler` and a panic hook in the client (`client/mod.rs` ~337-357). | None blocks one server per process. They do block two servers in one process, so in-process server tests rely on nextest's process per test (0644). |
| 4 | An "in-process call" is a channel message: `ApiRequestMessage { request, respond_to: std::sync::mpsc::Sender<String> }` (`api/mod.rs:86-92`). The loop owns `App` by `&mut` (`headless.rs:412`, select at `:627-644`). Answers are JSON strings. | Use that queue. Do not make herdr's API typed. Jocasta's `Client` (`herdr.rs:893`) gains an in-process transport that builds a `Request` from the same JSON. This removes connect, accept, a thread per call and the 0517 retry. |
| 5 | Five methods are served on the connection thread and need the stream: `events.subscribe`, `events.wait`, `agent.prompt`, `agent.wait`, `pane.wait_for_output` (`api/server.rs` ~428-488). | Expose `embed::call` for the app-dispatched arm (`handle_request`, ~`:551`). If `Herdr::prompt` uses `agent.prompt`, give `prompt_agent` a cancel probe in place of the stream. |
| 6 | `EventHub::push` only appends (`api/event_hub.rs:21-33`); nothing wakes a reader. | Add a `tokio::sync::Notify` to `EventHub`. The service awaits it and reads `events_after(seq)`. This replaces `WatchCore`'s subscription and tick (`herdr.rs` ~1900-2060). |
| 7 | The 100 ms plan poll (`thread_lifecycle.rs:31-33`, `:576-588`) waits on omp's session file, not on herdr. In-process calls do not remove it. | The forked omp hook can: it already handles `tool_execution_end` and `agent_end` (`integration/assets/omp/herdr-agent-state.ts:419-439`). Report the plan proposal from there through `pane.report_metadata`. Verify in omp 18.22 which event carries the `xd://propose` write; if none does, keep the poll (at most 2 s after a plan turn). |
| 8 | The 5 s tick (`service.rs:213`, `:514-515`) also drives steps herdr has no event for (hand commits, spend, catch-up reviews). | Split `Refresher`: herdr-fact steps run on events, debounced 150 ms; the rest on a 60 s tick. `refresh_ms` stays as the test seam. |
| 9 | The HUD is a `cmd /c type hud.txt` spawned every 5 s (`config/herdr.toml` `tab_bar_right`). | A status segment from the extension (seam in `app/tab_bar_status.rs`). Delete `hud.txt` if nothing else reads it. |
| 10 | The client shell draws chrome from `ClientShellSnapshot` (`protocol/wire.rs:916-952`), a frozen codec (`protocol/endpoint.rs:1-9`, upstream `AGENTS.md` "Stable client endpoint contract"). | Do not touch the snapshot. Append one opaque `Extension(Vec<u8>)` variant to each of the private `ServerMessage` and `ClientMessage` (`wire.rs:464`), carrying Jocasta's existing JSON messages (`frontier/protocol.rs`). Offset `PROTOCOL_VERSION` (`wire.rs:20`, 22 today) so it never collides with upstream. |
| 11 | Questions today rely on a one-time code in the popup's env, a reader secret and a held HTTP request (0519, 0558, 0578; `plugin.rs`, `popup_routes.rs`). | With a native overlay the server addresses the foreground client over herdr's private client socket: no code hand-off, no secret, no hold. Detach or Escape is the fallback. The typed letters (0568) stay. An overlay is not a pane, so `pane.send_keys` cannot reach it. |
| 12 | `today` is a split pane beside the boss (65/35). | Draw it as a right dock at 35% of the pane area, shown in the boss's workspace only. Seams: `ClientShellLayout` (`client/shell/state.rs:57`), `render_shell` (`client/shell/render.rs`), `composition.rs`. The client already reports its pane area, so panes resize. Visible change: section 5. |
| 13 | The five thread keys are `plugin_action` commands (`config/herdr.toml`), each spawning `jocasta plugin action …`, which posts over HTTP. | Register a built-in plugin `jocasta` in memory with the five actions. The server intercepts that id at `PluginActionInvoke` and calls the extension. `herdr.toml` syntax and the command menu are unchanged. |
| 14 | Dirs: `app_dir_name`, `config_dir`, `state_dir` (`config/io.rs:22-42`), `config_path` (`:195`). Sockets, session file and logs live under the config dir (`session.rs:163-187`). On Windows the pipe name is the path string plus a marker file at that path (`ipc.rs:51-80`). | One override set at process start (`embed::set_dirs`): data under `<data>/herdr/`, config file `~/.jocasta/agent/herdr.toml`. Use the default session. Unix socket paths must stay under 104 bytes: tests set `HERDR_SOCKET_PATH` and `HERDR_CLIENT_SOCKET_PATH` to short temp paths. |
| 15 | The daemon is spawned as `<exe> server` with the caller's env (`server/autodetect.rs`, `build_server_daemon_command`). The benchmark needs plain herdr sessions (`bench/lib/launch.rs`). | The service attaches only when `JOCASTA_SERVICE=1` is in the server's env, set by bare `jocasta` before launch and inherited by a handoff import child. `jocasta --session x server` is a plain herdr server. Bare `jocasta` clears inherited `HERDR_SOCKET_PATH`, `HERDR_CLIENT_SOCKET_PATH` and `HERDR_SESSION`. |
| 16 | herdr's CLI is hand-rolled: `cli::maybe_run` matches `args[1]` (`cli.rs:101-136`), and the rest is in `main` (`main.rs:511-806`); clap is used for help only (`cli/spec.rs`). Jocasta's router is clap derive (`shared/route.rs:43-121`). | `route()` checks the first word against herdr's word list and returns `Route::Herdr(args)`, which runs before any tokio runtime exists. Jocasta's hidden `plugin` and `service` words die, so `plugin` becomes herdr's. Dropped words: `update`, `channel`, `machine`, `--remote`, `--machine`, the two bridge roles. |
| 17 | On Windows the exe is re-invoked for toast activation: `platform::maybe_activate_desktop_notification` is the first thing `main` does (`main.rs:519-522`). | Call it before clap in `entry::main`, or a toast click becomes a usage error. |
| 18 | `worktree`: herdr has `list [--workspace ID \| --cwd PATH]`, `create`, `open`, `remove --workspace ID`, always JSON (`cli/worktree.rs:5,64,157,247,302`). Jocasta has `list \| remove <path>` (`shared/thread_cli.rs:34`). Bare `list` collides. | herdr's shapes win, since the CLI is the plugin API. Jocasta's leftover list becomes `worktree leftover`; `remove <path>` (positional) stays Jocasta's. Changes 0544: section 5. |
| 19 | Plugins get `HERDR_BIN_PATH` from `platform::launch_executable()` plus `HERDR_SOCKET_PATH`, `HERDR_ENV`, `HERDR_PLUGIN_*` (`app/api/plugins/runtime.rs:40-75`). Installs live in `config_dir()/plugins` (`plugin_paths.rs:5-7`). The API schema is embedded (`cli/api.rs:1`). | Compatible as long as the `Method` enum and schema stay whole and `build_info::version()` keeps herdr's number for `min_herdr_version`. Rule: wire and schema types stay; dropped behaviour answers `not_supported`. Plugins are installed into Jocasta's dir with `jocasta plugin install`. |
| 20 | Live handoff is Unix only: `server/handoff.rs` is `#[cfg(unix)]` throughout, `bootstrap.rs:222-225`. Trigger: `server live-handoff [--import-exe]` (`cli/server.rs:15`, `:205-232`), then `perform_live_handoff` (`headless/lifecycle.rs:25`). | `install.sh` runs it after the swap. The service needs `before_handoff` (pause timers, release the relay port) and adoption of live panes on start. The relay port is fixed by default (`overlay.rs` `RELAY_PORT`), so panes reconnect. Windows upgrades restart the server. |
| 21 | herdr's `.cargo/config.toml` sets `+crt-static` for MSVC and its `Cargo.toml` holds `[patch.crates-io] portable-pty`. Neither is read from a nested member. | Move both to the root. Static CRT then applies to rusqlite's bundled SQLite and rustls' C code; phase 1's gate proves the link. |
| 22 | "Packaging dropped" contradicts Windows: patch 0001 of the vendored portable-pty loads a pinned Microsoft ConPTY from `conpty/` beside the exe and otherwise falls back to kernel32's (`vendor/portable-pty.patches.md`, `packaging/windows/conpty.json`). | Keep `packaging/windows/` and `scripts/package_windows_conpty.*`. v1 runs on the system ConPTY until he approves the download: section 5. |
| 23 | There is no `omp.toml` among the 22 detection manifests. | Keep `pi.toml` and the omp and pi hooks; live-check that an omp pane still reports state. Keep the `detect::Agent` enum whole (it reaches config, sound and resume). |
| 24 | `crates/herdr/AGENTS.md:301-329` tells agents on a fork to refuse implementation work, and `build.rs` prints the same as a cargo warning. omp loads nested `AGENTS.md`. | Delete `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `build.rs`, `.agents/`, `.pi/`, `.zed/`, `.githooks/`, `.github/` in the commit right after the subtree lands, before any other work in that folder. |
| 25 | "Upstream history kept" brings upstream authors into `git log`; the repo's pre-push rule requires every address to be the repo identity. | Contradiction: section 5. |
| 26 | herdr is Apache-2.0; the repo is MIT. | Keep `crates/herdr/LICENSE`; `FORK.md` states the changes (Apache §4b); check `deny.toml`'s licence list. |
| 27 | Jocasta's ceilings (0610: 1,000 lines a file, 120 a function) cannot hold for herdr (`pane.rs` 245 KB, `terminal/state.rs` 225 KB). | `tests/tree_checks.rs`, Biome, knip, tsc, the Bun test glob, the docs tests and the scorecard exclude `crates/herdr/**`. New fork files (`embed.rs`) obey the ceilings. `cargo fmt --check` stays root-package only. |
| 28 | The snapshot replaces `config.yml` whole. His own keys outside the snapshot, model roles among them, would go. | The snapshot carries the decided roles (0590, 0591). It still holds only deltas from omp's defaults (principle 3). The diff shows removals before his `y`. |
| 29 | The trials passed 144 of 144 on the 12 cases (`state-of-the-art.md` §13), so they cannot show quality; 0620 left thinking and the advisor to the follow-up study. "Every value" would also try values that send data out or weaken the guard. | Two classes are never adopted from a trial alone: section 4. |
| 30 | herdr's integration tests spawn the crate's own bin (33 files under `tests/`); upstream's Python script tests go with `scripts/`. | Record the upstream suite's results on his PC before any trim (`crates/herdr/TESTS-BASELINE.md`). Later gates allow no new failures. |
| 31 | The service on herdr's runtime could starve it with blocking work (SQLite, file reads). | Give the service its own tokio runtime (2 workers) inside the server process. `api_tx`, `EventHub` and `ServerStop` are runtime-agnostic. |
| 32 | With one process, 0518's revival of a dead herdr server, 0582's sparing of it and H3's separate faults no longer exist. | Bare `jocasta` relaunches the server on a lost connection (at most 3 times in 60 s). 0534's hung-holder takeover stays, by a `<data>/server.pid`. H3: section 5. |
| 33 | CI builds from scratch on three runners with a 45-minute limit (`.github/workflows/ci.yml`). | Add Zig through mise, run with `--workspace`, raise the limit to 90. No cache action (a new dependency). |

## 2. Shape after the build

```
jocasta (client, his terminal)            jocasta server  (JOCASTA_SERVICE=1)
  herdr client loop                         herdr HeadlessServer loop (main thread)
  + ShellExtension: today dock,      <-->   + ServerExtension on its own runtime:
    question overlay, key routing            the Jocasta service (threads, gates,
  (client socket: Extension msgs)            relay, pollers, ledger, history, HTTP
                                             for canvas, doors, mcp, bench feed)
omp panes, plugins, CLI  --- socket API (unchanged) --->  api_tx queue  <--- in-process Herdr
```

Fork seams in kept herdr files: `lib.rs` and `embed.rs` (new), `main.rs`, `server/headless/bootstrap.rs`, `server/headless.rs` (extension messages, client attach and detach), `headless/lifecycle.rs`, `api/event_hub.rs`, `api/server.rs`, `config/io.rs`, `protocol/wire.rs`, `client/mod.rs`, `client/shell/{state,render,composition,input}.rs`, `app/tab_bar_status.rs`, the plugin action invoke path, `crates/ghostty-vt/build.rs`.

## 3. Phases

### Phase 0: baseline
- **Do:** run the gate at HEAD. mise refuses `mise.toml` until he runs `mise trust`; run the pinned tools directly, as `status.md` describes. Record today's numbers with `bench/h5_startup.rs`, `h6_memory.rs`, `h9_footprint.rs` logic: process count, idle memory of service, herdr server, client and `today`, startup time, binary size.
- **Gate:** the gate passes. No commit.

### Phase 1: subtree and workspace, no behaviour change
- **Do:**
  - `git subtree add --prefix=crates/herdr https://github.com/herdrdev/herdr.git v0.9.5`. Full history; the local clone is shallow, do not use it.
  - Immediately delete the agent and repo dressing of finding 24, plus `flake.*`, `nix/`, `rust-toolchain.toml`, `.cargo/`, `Cargo.lock`, `SPONSORS.md`, `README.zh-CN.md`, and the images under `assets/` (keep `assets/sounds/`).
  - Root `Cargo.toml`: `[workspace] members = [".", "crates/herdr", "crates/herdr/crates/ghostty-vt"]`, `default-members = ["."]`, the `portable-pty` patch. Remove `[workspace]`, `[patch]` and `build =` from herdr's manifest.
  - Root `.cargo/config.toml`: the MSVC `crt-static` flag.
  - `mise.toml`: `zig = "0.16.0"`, `CARGO_BUILD_JOBS`; run `mise lock`. He must `mise trust` again.
  - One root `Cargo.lock`. The 7-day rule resolves on its own; pin `interprocess`, `crossterm`, `bincode` and `portable-pty` to herdr's locked versions.
  - Exclusions of finding 27. `deny.toml` licences.
- **Gate:** `cargo build -p herdr` on Windows and in WSL; herdr's suite recorded as the baseline; the root gate still passes.
- **Commit:** `fork: herdr v0.9.5 as a subtree at crates/herdr, one workspace`.

### Phase 2: trim, still a plain herdr
One drop at a time, each followed by `cargo check -p herdr --all-targets`.

| Drop | Files |
|---|---|
| Remote and machine API | `src/remote/`, `remote.rs`, `cli/machine.rs`, `cli/spec/machine.rs`, `cli/target.rs`, `platform/remote_bridge*.rs`, `platform/ssh_agent.rs`, `client/endpoint/ssh_metadata.rs`, `client/shell/machine_diagnostics.rs`; tests `remote_attach.rs`, `machine_api.rs`, `machine_setup.rs`. Keep the client's endpoint registry with Local only; do not rewrite it. |
| Update checker and kin | `update.rs` (134.5 KB), `detect/manifest_update.rs`, `release_notes.rs`, `ui/release_notes.rs`, `product_announcements.rs`, `ui/onboarding.rs`, the `update` and `channel` words, the `[update]` and `onboarding` config keys. Snapshot fields stay as constants. |
| Render benchmark | `server/render_scale_benchmark.rs`. Keep `render_prof.rs` (called from hot paths). |
| Other agents | 21 of 22 `detect/manifests/*.toml` (keep `pi.toml`); `integration/assets/` except `omp/` and `pi/`; their config editors (`claude_settings.rs`, `opencode_config.rs` and the like). |
| Packaging and docs | `distribution/`, `docs/versions/`, `docs/preview/`, `docs/next/` except `api/herdr-api.schema.json`, `scripts/` except the ConPTY and vendor scripts. |

- **Kept whole:** plugins, kitty graphics, live handoff, worktrees, notifications and sound, persistence, sessions, `vendor/`, `skills/herdr/SKILL.md`, `tests/fixtures/` (included by unit tests in `raw_input.rs`, `input/parse.rs`, `persist/`).
- **Also:** write `crates/herdr/FORK.md` (dropped, kept, seams, the merge procedure: `git subtree pull`, then `git rm` every modify/delete conflict).
- **Gate:** herdr's suite equals the baseline minus the dropped parts' tests, on Windows and WSL. A headless smoke of the crate's bin in a throwaway session: split a pane, run `omp`, `plugin list`, `server stop`. Report `.rs` bytes and binary size before and after.
- **Commit:** `fork: herdr trimmed`.

### Phase 3: library, one binary, merged CLI, own dirs
- **herdr:** `lib.rs`, `embed.rs` (`Dirs`, `ServerExtension`, `ShellExtension`, `ServerHost`, `call`), `run_server_with`, `run_client_with` over `run_client_with_mode` (`client/mod.rs:175`), the dirs override.
- **Jocasta:** `entry.rs` and `route.rs` per findings 16-18; the merged help grafts herdr's `spec::command()` subcommands. `tests/herdr_config.rs` runs `jocasta config check` on the built binary, which ends its dependence on an installed herdr.
- **Unchanged this phase:** bare `jocasta` still starts the old service and the external herdr.
- **Gate:** new `tests/entry.rs` cases: a herdr verb with no server prints herdr's `server_not_running` JSON and exits 1; `jocasta --session t server` with `jocasta --session t pane list` round-trips; `jocasta workspace --help` is herdr's; every existing Jocasta word behaves as before. One test links herdr's `tests/fixtures/plugin-smoke` and runs an action that calls `$HERDR_BIN_PATH pane list`.
- **Commit:** `one binary: herdr's verbs in jocasta`.

### Phase 4: one server
- **Moves into the server:** everything `run.rs::run_service` starts, as a `ServerExtension` in a new `src/frontier/server/embedded.rs`.
- **Dead now:**
  - `run.rs`; `instance.rs` except the hung-holder takeover; the `Service` route.
  - In `herdr.rs`: `connect_herdr`'s start path, `reviver`, `start_server`, `start_and_wait`, `stop_herdr_session`, `herdr_session_answers`, `find_herdr`, `herdr_version_warning`, `REVIVED_AT`, `REVIVALS`, `READS`, the `WatchCore` tick.
  - In `surface.rs`: `start_service`, the 50/250 ms health poll, `attach` by argv, `stop_herdr`, `watch_service_stop`, `herdr_up`, `AttachedSurface`'s pid files (the server knows its clients).
  - `setup/mise.rs`'s herdr pin; `hud.txt`.
- **Stays:** the pipe transport and `FakeHerdrServer` (the benchmark and tests use them); `FakeHerdr` for the service's 1,469 tests; `service_file.rs` and the HTTP doors for `jocasta mcp`, `thread`, `push-hook`, `pane-check` and the bench feed.
- **Also:** the event-driven refresh, the status segment, the plan signal from the hook, and stop ordering (list pane processes before `server.stop`, end them after `server.run()` returns; 0486).
- **Gate:** the gate; `mise run smoke` against the embedded server; a new PTY test (portable-pty as a dev-dependency) that runs bare `jocasta` at 120×40 and sees the boss workspace through the API; a handful of in-process tests with a real server and a temp data dir (thread start makes a pane; a pane exit reaches the service without a tick).
- **Commit:** `one server: the service inside herdr's`.

### Phase 5: native `today`, questions and keys
- **Build:** findings 10-13. `today/view.rs` and `ask/view.rs` keep their render logic behind a `ShellExtension` adapter. The server side presents each question to the foreground client and presents it again when a client attaches.
- **Deleted:**
  - `config/herdr-plugin/`; `today/main.rs`; `ask/main.rs` with its tests; `tui.rs`'s `run_pane` and `PaneProgram`.
  - In `frontier/client.rs`: `state_feed`, `question_view`, `answer_question`, `hold_question`, `post_action`.
  - `route.rs`'s `Plugin`, `PluginPart`, `Part`, `ACTION_WORDS`; `entry.rs`'s `run_plugin`, `run_plugin_action`.
  - `plugin.rs`'s `PluginPanes`, `apply_agent_view`'s startup route and the popup opener with its retries.
  - `Herdr::open_plugin_pane`, `close_popup`, `set_split_ratio` and their types, if nothing else calls them.
  - The hold and reader-secret half of `popup_routes.rs` and `popups.rs`.
- **Check first:** how the benchmark answers questions. No file under `bench/` or `evals/` uses `POPUP_PATH`; if a script types into the popup pane, add a bench-only door behind the existing bench flag.
- **Setup:** drop the plugin-link migration; one cleanup step unlinks the old plugin and removes `~/.jocasta/agent/herdr-plugin/`. The `herdr.toml` template loses `[update]`, `onboarding` and `{{hud}}`.
- **Gate:** the view tests on the adapters; `e2e_keys_tests.rs`, `e2e_allow_tests.rs`, `server_popups_tests.rs`, `plugin_tests.rs` updated as contract tests; the PTY test sees the dock's heading and answers an Allow with the typed code. Measure processes and memory against phase 0.
- **Commit:** `today and questions drawn natively; the plugin is gone`.

### Phase 6: the omp snapshot
- **Layout:** `config/omp/` mirrors `~/.omp/agent`: `config.yml` (from `config/omp-config.yml`), `keybindings.yml`, `WATCHDOG.md`, `lsp.json`, `mcp.json`, `skills/`, `secrets.yml`, and the herdr hook. The hook has one source, the fork's asset, embedded at build. Per-pane overlays (`guard.yml`, `boss.yml`, `heavy.yml`, `headless.yml`, `ponytail.md`, `pane-check-extension.ts`) stay where they are.
- **Setup:** one step replaces the eight that write into omp's folder (`plan_mcp`, `plan_omp_settings`, `plan_herdr_integration`, `plan_mise_skill`, `plan_keybindings`, `plan_watchdog`, `plan_lsp`, `plan_secrets` in `setup_migrations.rs`). Per file: diff (`setup/diff.rs`), one `y`, a backup under `~/.jocasta/agent/setup/omp-backup/<UTC stamp>/`, then the write. `secrets.yml` only when missing. The Windows folder fence (0594) stays as a text patch.
- **Uninstall:** restores a backup while the file is still the snapshot's text.
- **Dead:** `merge_omp_settings` and its types; `omp_yaml`, `NUMBER_SCAN`, `omp_yaml_tests.rs` and `omp_yaml_corpus.json` (429 KB) once `config_file.rs` and the Pi-settings migration no longer call them; the clean-up migrations for earlier versions (`plan_jocasta_yaml`, `plan_autostart_off` with `run_value.rs`, `plan_graphiti_removed`, `plan_whiteboard_boss_only`). New steps take numbers 18 and up; old markers are ignored.
- **Gate:** contract tests: the diff is shown, nothing is written without `y`, backups exist, a second run is a no-op, logins, sessions, memory and history are byte-identical. A dry run on a sandbox copy of his real `~/.omp/agent`, its diff saved in the ledger. Never write to his real folder.
- **Commit:** `the omp snapshot`.

### Phase 7: the settings trials
Section 4. **Commit:** the snapshot's values, `docs/research/settings-trials-2.md`, the pinned omp version with plan 5.4's seams re-checked.

### Phase 8: upgrade path, three OSes, docs, records
- **Upgrade:** the handoff hooks of finding 20. `install.sh` runs `jocasta server live-handoff --import-exe <new>` when a server answers. `install.ps1` says to restart.
- **Build and CI:** `scripts/build.ts` packs the one binary. `ci.yml` per finding 33. Zig in WSL through mise.
- **Bench:** `bench/lib/launch.rs` starts `jocasta --session <name> server`; the bench compiles and its unit tests pass. No VM runs in this build.
- **Docs:** plan sections 5.1-5.5 and appendix I (words, env, seams), `AGENTS.md`'s package map and "Where we are", `docs/manual.md`, `README.md`, `CHANGELOG.md`, `docs/status.md`, `docs/testing.md`, records 0649+.
- **Gate:** the full gate on Windows and in WSL; herdr's `tests/live_handoff.rs` in WSL plus one Jocasta test (handoff with a stand-in omp pane; the service adopts the thread). macOS is proven only by CI after his push.
- **Commit:** `v1: the fork and the snapshot`.

## 4. The settings trials

**Harness.** `jocasta-bench evals`: `run_trial` (`evals/runner.rs`), variant overlays (`evals/variants.rs`), the pacer (`evals/pacer.rs`), `compare` (`evals/compare.rs`), prices from omp's `usage.cost` through `src/shared/ledger.rs`. Add a `settings` subcommand that owns the ladder below. The 2026-11-15 scripts in `D:/jocasta-spikes/sota/settings-trials/` are TypeScript against the removed runner: read them, do not reuse them.

**New row fields** on `ResultRow` (`evals/runner.rs` ~157): `costUsd`, `peakTreeBytes` (Windows: the job object's `PeakJobMemoryUsed`; Linux: cgroup `memory.peak`, else summed `ru_maxrss`), `minAvailableBytes` (machine available memory sampled every second, as `pollers/machine.rs` reads it).

**Memory floor.** Trial concurrency 1. Start a trial only with 4 GB available. If available stays under 1.5 GB for 5 samples, end the trial's tree and mark it `infra: memory`. No cargo job runs during trials.

**Step A: enumerate, no model.** Dump omp's registry (`packages/coding-agent/src/config/all-settings.ts` in the install; load it as `bench/h1-guard/omp-verdicts.ts` loads omp's modules) to `settings.jsonl`: key, type, default, enum values, tab. All 629 get a row and one class by fixed rule:

| Class | Rule | Treatment |
|---|---|---|
| P, pinned | sends data out, or weakens the guard or approvals (the keys of 0398, 0451, 0593 and `guard.yml`) | checked at the pinned value only; never adopted from a score |
| J, per pane | set by an overlay under a record (`memory.backend`, `plan.*`, `collab.*`) | checked at the decided value |
| F, free-form | string, path, list or object with no enumerable values | checked at the default and the shipped value |
| Q, quality trade | thinking level, model roles, advisor, compaction order, `read.summarize`, `snapcompact` | trialled, reported, not adopted on these 12 cases (0619, 0620) |
| C, candidate | the rest | all values: both booleans, every enum value, numbers at default ×0.5 and ×2 |

**Step B: no-model check of every (key, value).** About 1,600 omp launches, one at a time, no spend.
- Fixed tokens: omp in RPC mode with a scratch agent folder and a dummy key, system prompt and tool list counted with `omp toks` (0619's tier 0a).
- Replay: a loopback stand-in for the provider (axum is in the tree) that scripts a fixed 10-turn tool sequence (read a large file, grep, bash, edit, task) on a fixture repo and hashes every request body. Verify how omp 18.22 takes a provider base URL; if it cannot, fall back to fixed tokens plus a source check of where the key is read.
- Recorded per run: request hashes, wall time, peak tree memory, exit.
- Verdict: identical hashes, with time and memory inside the spread of 5 base runs, is **inert** and the default stays unwritten. Anything else is **live**, ranked by effect size.

**Step C: real trials, live C and Q arms only.** Opus 5.5 at high, base as 0619 defined it (a Jocasta pane's config with the advisor pinned off).
- Per-trial cap `--max-cost 1.00`. The run stops at $95 measured.
- The arm that goes first rotates by case, for the prompt cache.
- Budget, at 0619's $0.09 a base trial (24 trials for $2.15):

| Use | Trials | About |
|---|---|---|
| Base, 12 cases × 3 reps | 36 | $4 |
| Screen by successive halving: every arm on 4 cases; top third +8 trials; top ninth to 24 | 8 per arm, 90 arms | $65 |
| Confirmation | 72 | $7 |
| Forward-selection reserve | | $10 |
| Infra retries and costlier arms | | $12 |

- An arm that fails a case base passes is out. If step B leaves more than 90 live arms, the ladder takes the 90 largest effects and the ledger names the rest "live, not afforded, default kept".

**Step D: combine and confirm.** Winners are C arms whose 24-trial bootstrap interval over cases excludes zero on cost or wall time at equal pass. Take one per subsystem and combine them into one overlay. Confirm combined against base, 12 cases × 3 reps, interleaved, with `compare`'s keep rule and paired statistics. If combined is not better than the best single arm, add winners one at a time in effect order (12 trials each) within the reserve. The result goes into `config/omp/config.yml` as deltas, with one ledger line per setting: class, evidence, verdict.

## 5. Pruning and deduplication targets seen today

| Target | Size | When |
|---|---|---|
| herdr drops: update checker 172 KB, remote 250 KB, manifest updates 34 KB, machine API 30 KB, render benchmark 24 KB, release notes 20 KB, announcements 11 KB, 16 integrations and 21 manifests, `distribution/` (`latest.json` alone 173 KB), docs | about 0.6 MB of `.rs` plus assets | Phase 2 |
| `setup/omp_yaml_corpus.json` and the Bun-YAML writer in `omp_settings.rs` (16.2 KB, a 35-state scan table) | 445 KB | Phase 6 |
| Eight omp-folder migrations become one; four old clean-up migrations; `run_value.rs`. `setup_migrations.rs` 27.8 KB, `setup_tests.rs` 64.3 KB, `uninstall.rs` 41.8 KB and `uninstall_tests.rs` 34.9 KB shrink | about half of 170 KB [INFERENCE] | Phases 5-6 |
| `today/main.rs` 7.7 KB, `ask/main.rs` 8.0 KB with tests 4.7 KB, `tui.rs`'s program half (of 13.4 KB), `frontier/client.rs`'s feed and question half (of 12.4 KB), `config/herdr-plugin/` | about 35 KB | Phase 5 |
| `plugin.rs` 23.4 KB and `plugin_tests.rs` 24.7 KB: pane healing, popup opener, retries | about 60% | Phase 5 |
| `popup_routes.rs` 14.0 KB with tests 22.4 KB, `popups.rs` 15.1 KB: hold and reader secret | about half | Phase 5 |
| `herdr.rs` 77.6 KB: revival, server start and stop, `find_herdr`, version warning, `READS`, `WatchCore` tick | about 12 KB, and `herdr_tests.rs` 50.2 KB shrinks with it | Phase 4 |
| `run.rs` 5.1 KB; `instance.rs` 8.0 KB with tests 5.8 KB; `surface.rs` 17.3 KB with tests 25.2 KB | about 40 KB | Phase 4 |
| Timers: the 5 s tick, the 5 s `cmd` spawn for the HUD, the 50/250 ms service poll, the `WatchCore` tick; the 100 ms plan poll if the hook can report it | | Phases 4-5 |
| Processes: 4-5 today (service, herdr server, client, `today`, `ask`) down to 2 | | Phase 5 |
| Candidates to measure only: the JavaScript-number serializers in `frontier/protocol.rs` (written bytes are decided formats, remove only proven-unused helpers); `tokio-tungstenite` (still used by the bench feed and test helpers); two loggers (`shared/log.rs`, herdr's tracing) | | After v1 |

## 6. For Zhivko, after all

The builder notes each, takes the default, and carries on.

1. **Upstream history against the identity rule.** The subtree brings herdr's authors into `git log`. Default: commit locally, no push, until you scope the rule to commits outside the subtree or choose a squashed subtree.
2. **Microsoft's ConPTY runtime.** Upstream ships a pinned, hash-checked copy beside the exe. Default: Windows' own ConPTY until you approve the download at packaging time.
3. **`jocasta worktree list`.** Default: herdr's JSON listing; your leftover list moves to `jocasta worktree leftover`. Changes 0544.
4. **`today` as a right dock**, 35% wide, in the boss's workspace only, no longer a pane.
5. **The fork is outside Jocasta's ceilings** (0610); only the new fork files follow them.
6. **The snapshot's `config.yml` carries your model roles** (0590, 0591); any other key of yours not in it is removed after the diff and your `y`.
7. **Trial limits.** Privacy and guard settings stay pinned whatever a trial says; thinking, roles, advisor and compaction are reported but not adopted on the 12 cases (0620).
8. **H3.** With one server, the separate "service" and "herdr server" faults become one fault.
9. **herdr's crates enter the lock** (about 25 direct ones, implied by the fork), and CI's limit goes to 90 minutes.
10. **herdr's verbs keep upstream's usage and error texts**, which still say "herdr".
