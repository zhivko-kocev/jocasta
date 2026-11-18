# This fork

herdr v0.9.5 (`github.com/herdrdev/herdr`, tag `v0.9.5`, commit `2ecf19a9`), Apache-2.0, brought into Jocasta's repo with
`git subtree add --prefix=crates/herdr … v0.9.5 --squash` on 2026-11-18. Jocasta builds it into its one `jocasta`
binary; it is not shipped as `herdr`. Changed files are listed below, as Apache-2.0 §4(b) asks; `LICENSE` is herdr's.

## Removed

- The repo's agent and contributor dressing: `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `.agents/`, `.pi/`, `.zed/`,
  `.githooks/`, `.github/`, and `build.rs` (it only printed the contributor policy and named three build variables).
- Its own toolchain and lock, which the workspace root holds: `rust-toolchain.toml`, `.cargo/`, `Cargo.lock`; the Nix
  files (`flake.*`, `nix/`), `justfile`, `SPONSORS.md`, `README.zh-CN.md`, and the README's images under `assets/`
  (the sounds stay).
- Remote and machine access: `src/remote/`, the `machine` CLI and its spec, the remote bridge and SSH-agent platform
  files, `client/endpoint/ssh_metadata.rs`, `client/shell/machine_diagnostics.rs`, and the tests `remote_attach.rs`,
  `machine_api.rs`, `machine_setup.rs`. `remote.rs` stays as three constants the client still reads; a saved SSH
  endpoint answers `Unsupported`, and `server.ssh_agent.register` answers `not_supported`.
- The self-updater: `update.rs` keeps only `Version` and the install hint (137,701 B to 2,835 B); the `update` and
  `channel` words, `checksum.rs`, and the manifest updater `detect/manifest_update.rs` (replaced by the local
  `detect/manifest_version.rs`). The `[update]` and `[remote]` config tables are still parsed, and ignored, so an
  existing config raises no unknown-key notice.
- The render benchmark `server/render_scale_benchmark.rs`.
- Other agents: 21 of the 22 detection manifests (only `pi.toml` stays; omp is detected through it), the 21
  integration assets other than `omp/` and `pi/` with their install code and config editors (`claude_settings.rs`,
  `opencode_config.rs`, `config_edit.rs`, `config_file*`, `command.rs`, `file_ops.rs`, `version.rs`,
  `test_support.rs`, `platform/windows/config_backup.rs`). A dropped target answers `unknown integration target`. The
  `IntegrationTarget` and `Agent` enums, the API schema and every wire type stay whole. Codex's manifest survives as a
  test fixture (`tests/fixtures/codex-manifest.toml`) for the loader's tests.
- Packaging and docs: `distribution/`, `docs/` except `docs/next/api/herdr-api.schema.json` (embedded by `cli/api.rs`),
  `scripts/` except the ConPTY, vendor and libghostty scripts, `CHANGELOG.md`.

Kept whole: plugins, kitty graphics, live handoff, worktrees, notifications and sound, persistence, sessions,
`vendor/`, `skills/herdr/SKILL.md`, `tests/fixtures/`, the local release notes and announcements, onboarding.

## Changed

### Trim edits

The removals above reached into these kept files; each edit only deletes a dropped feature's code path, help line,
event, field or test: `Cargo.toml` (no `[workspace]`, `[patch.crates-io]` or `build`, and `publish = false` for
cargo-deny's path-crate rule; the root's workspace lists this
crate and `crates/ghostty-vt` and patches `portable-pty`; the MSVC static C runtime is in the root's
`.cargo/config.toml`), `api/client.rs`, `api/server.rs`, `app/{actions,api,mod,runtime,state}.rs`,
`app/api/worktrees.rs`, `cli.rs`, `cli/{integration,plugin,server,spec,status,target,worktree}.rs`,
`client/{endpoint,handshake,mod,shell,shell_runtime,transport}.rs`, `client/endpoint/{catalog,supervisor}.rs`,
`client/shell/{composition,endpoint_sidebar,endpoints,input,render,state}.rs`, `client/shell/tests/endpoints.rs`,
`config.rs`, `detect/{mod,manifest}.rs`, `detect/manifest/tests.rs`, `events.rs`,
`integration/{actions,env,mod,registry,targets,tests,types}.rs`, `ipc.rs`, `logging.rs`, `noninteractive_process.rs`,
`pane.rs`, `platform/{fallback,linux,macos,mod,unix_common,windows}.rs`, `product_announcements.rs`,
`release_notes.rs`, `server/{mod,headless}.rs`, `server/headless/notifications.rs`, `server/headless/tests/mod.rs`.

Also `src/pane/terminal.rs`, the test `render_keeps_halfwidth_katakana_voiced_tail_empty`: it checks what the host
terminal is sent (no write over the wide tail) rather than the test backend's copy, because the workspace's ratatui
0.30.2 counts a halfwidth voiced mark's cell and so skips the tail that 0.30.0 wrote as empty.

### Seams for Jocasta

- `src/lib.rs` (new; upstream's `src/main.rs` with `pub mod embed` and a `pub fn main`) and a thin `src/main.rs`, the
  `herdr` bin herdr's integration tests spawn.
- `src/embed.rs` (new): `Dirs`, `set_dirs`, `version`; `OMP_HOOK` and `OMP_HOOK_FILE`, the omp integration's asset and
  file name for Jocasta's omp snapshot; `ServerExtension`, `ServerHost` (an in-process `stop` and a
  `send` to the clients by `Target`: one client or every shell), `set_server_extension`, the hooks a server in
  Jocasta's process runs its service through (`started`, `client_joined` with the client's peer process,
  `client_left`, `last_client_left`, `client_message`, `action`, `stopped`); `peer_pid`, the process at the other end
  of a client's connection as the OS reports it; `ServerHost::herdr_changed`, woken by the events Jocasta's refresh
  reads (`herdr_event`); `ShellExtension`, `Reply`,
  `set_shell_extension`, `DOCK_COMMAND`, what herdr's client draws and takes keys for in Jocasta's process (the dock,
  a question over everything, the tab bar's status segment); `set_exit_hook`, run before herdr's client ends the
  process with a failure. Both sides talk in `EndpointControl {kind, data}` messages whose kind starts with
  `jocasta.` (`KIND_PREFIX`), so the wire protocol and its version are upstream's.
- `src/config/io.rs`: `config_dir`, `state_dir` and `config_path` take `embed`'s dirs first (`HERDR_CONFIG_PATH` still
  wins for the config file).
- `src/server/headless/bootstrap.rs`: `run_server` tells the extension the server listens (after the plugins' startup
  hooks), with its `ServerHost`, and that it stopped (after its loop's runtime returns).
- `src/server/headless.rs`: `extension_host` (the stop `server stop` does, callable in-process, and the outbound
  channel the loop drains into `EndpointControl` messages); a shell client's attach and leave reach the extension, and
  the last one's leave unless the server is stopping; a client's `jocasta.` message reaches it as
  `ServerEvent::ExtensionMessage`.
- `src/server/client_transport.rs`: `ServerEvent::ExtensionMessage`, read from a client's `EndpointControl` of kind
  `jocasta.*` before upstream's endpoint controls; a shell's peer process noted at its handshake.
- `src/app/api/plugins/mod.rs`: a key bound to a `jocasta.<action>` plugin action goes to the extension with the
  focused pane, before the plugin registry is read (`plugin.action.invoke` over the socket is upstream's).
- `src/api/event_hub.rs`: `EventHub::push` hands each event's kind to `embed::herdr_event` first.
- `src/integration/mod.rs`: `OMP_EXTENSION_INSTALL_NAME` and `OMP_EXTENSION_ASSET` are `pub(crate)`, for `embed`.
- `src/client/mod.rs`, `src/client/shell.rs`, `src/client/shell/{extension,actions,composition,config,input,mouse,
  state,tabs}.rs`: the client hands `jocasta.` messages to the shell extension and writes its replies back to the
  server; draws the dock beside the focused workspace's panes when the extension docks there, and a question over
  everything; gives the dock his keys on `DOCK_COMMAND` or a click in it, and every key, text and paste to a question,
  ahead of herdr's own overlays (onboarding, announcements, release notes); ticks it with the client's timers; puts its
  status at the left of the tab bar's right side.
- `src/client/mod.rs` (the client's failure exit) and `src/lib.rs` (a launch that failed) end the process through
  `embed::exit`, which runs the exit hook first.

### Vendored build dependency

- `vendor/translate-c/` (new): Zig's `translate-c` package (MIT, commit `4e879eb8aba615de112eabd1231ea6e01920cead`,
  17 files as Zig fetched them), and `vendor/libghostty-vt/build.zig.zon` names it by `.path` instead of its
  codeberg.org URL, so a build fetches nothing from codeberg. Its own dependency `aro` still comes from github.com.

## Taking an upstream release

`git subtree pull --prefix=crates/herdr https://github.com/herdrdev/herdr.git vX.Y.Z --squash`, then `git rm` every
file this fork removed that the release changed (a modify/delete conflict), and re-apply the seams listed above.
herdr's own suite (`cargo nextest run -p herdr`) and Jocasta's run before the merge is committed.
