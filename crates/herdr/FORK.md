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
- The expanded sidebar's spaces section (0659): its rows, hits, scroll, drag and reorder, section divider and split,
  worktree-group collapse, and the multi-machine "machines" list that shared its area. The agents list takes the
  sidebar's height under a one-row strip with `new` and `menu`. The collapsed sidebar's workspace markers stay.
  `[ui.sidebar.spaces]`, the split ratio and the client preferences' `sidebar_section_split`, `collapsed_groups` and
  `remote_collapsed_groups` are still read, and ignored; the server's session file and wire types keep their fields.
- The strip's `new · Local` button (0664): the expanded sidebar's top strip draws `menu` only.
- Workspace creation, rename and worktrees in the client (0665): `new_workspace`, the worktree create/open/delete
  overlays (`client/shell/{worktrees,worktree_overlays}.rs`), the workspace rename, their keys, buttons, right-click
  and mobile entries. The workspace right-click menu is Close alone. The `new_workspace`, `new_worktree`,
  `open_worktree`, `remove_worktree` and `rename_workspace` keys, `ui.prompt_new_workspace_name` and `onboarding`
  are still read, and ignored. The server's `worktree.*` API and the `herdr worktree` CLI stay.
- On the boss's workspace, his client's close workspace, new tab (key and `+`), split and close pane do nothing, and
  its tab close too (see the seams below).
- The release notes and "what's new", the "update ready" badge, the product announcement overlay, onboarding
  (`ui/{release_notes,onboarding}.rs`) and the settings' Integrations tab with its installs (0666). The menu's
  "detach" and the `detach` key (`prefix+q`, still read, and ignored) went; the menu ends with "quit Jocasta".
  Help lists Jocasta's `[[keys.command]]` entries first.

Kept whole: plugins, kitty graphics, live handoff, the server's worktree commands and API, notifications and sound,
persistence, sessions, `vendor/`, `skills/herdr/SKILL.md`, `tests/fixtures/`, the server's `release_notes.rs` and
`product_announcements.rs` with their wire fields and API methods (nothing in the client reads them).

## Changed

### Trim edits

The removals above reached into these kept files; each edit only deletes a dropped feature's code path, help line,
event, field or test: `Cargo.toml` (no `[workspace]`, `[patch.crates-io]` or `build`, and `publish = false` for
cargo-deny's path-crate rule; the root's workspace lists this
crate and `crates/ghostty-vt` and patches `portable-pty`; the MSVC static C runtime is in the root's
`.cargo/config.toml`), `api/client.rs`, `api/server.rs`, `app/{actions,api,mod,runtime,state}.rs`,
`app/api/worktrees.rs`, `cli.rs`, `cli/{integration,plugin,server,spec,status,target,worktree}.rs`,
`client/{endpoint,handshake,mod,shell,shell_runtime,transport}.rs`, `client/endpoint/{catalog,supervisor}.rs`,
`client/shell/{composition,config,context_menu,endpoint_navigation,endpoint_sidebar,endpoints,input,mobile,mouse,
preferences,render,sidebar,state,workspace_navigation}.rs`, `client/shell/tests/{agents_worktrees_notifications,
chrome_context,endpoints,keybindings_settings,popup_focus_projection,workspace_navigation}.rs`, `ui.rs`,
`ui/sidebar.rs`, `ui/sidebar/tokens.rs`,
`config.rs`, `detect/{mod,manifest}.rs`, `detect/manifest/tests.rs`, `events.rs`,
`integration/{actions,env,mod,registry,targets,tests,types}.rs`, `ipc.rs`, `logging.rs`, `noninteractive_process.rs`,
`pane.rs`, `platform/{fallback,linux,macos,mod,unix_common,windows}.rs`, `product_announcements.rs`,
`release_notes.rs`, `server/{mod,headless}.rs`, `server/headless/notifications.rs`, `server/headless/tests/mod.rs`.

0664–0666 also reached `app/{api,mod,state}.rs`, `client/shell/{actions,agent_sidebar,config,context_menu,
endpoint_sidebar,endpoints,extension,global_menu,input,input_source,mobile,mouse,overlay_input,overlays,render,
settings,settings_overlay,sidebar,tabs,text_editor}.rs`, `client/shell_runtime.rs`, `config/{io,keybinds,model,
write}.rs`, `input/{keybind_help,keybindings}.rs`, `ui.rs`, `ui/{scrollbar,widgets}.rs`, `update.rs`, `lib.rs` (the
config template), `workspace.rs` and `workspace/git/{discovery,mod}.rs`, and deleted `client/shell/{worktrees,
worktree_overlays}.rs` and `ui/{onboarding,release_notes}.rs`; the tests of the removed features went with them, and
`client/shell/tests/{boss_guard,global_menu}.rs` are new.

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
  process with a failure; `before_handoff`, `handoff_env` and `handoff_failed` (Unix), a live handoff's turns: the
  extension stops its service before the old server's clients leave and names the env the import server starts with,
  and starts it again when the handoff fails. Both sides talk in `EndpointControl {kind, data}` messages whose kind
  starts with `jocasta.` (`KIND_PREFIX`), so the wire protocol and its version are upstream's.
- `src/config/io.rs`: `config_dir`, `state_dir` and `config_path` take `embed`'s dirs first (`HERDR_CONFIG_PATH` still
  wins for the config file).
- `src/server/headless/bootstrap.rs`: `run_server` and `run_handoff_import_server` tell the extension the server
  listens (after the plugins' startup hooks), with its `ServerHost`, and that it stopped (after its loop's runtime
  returns).
- `src/server/headless/lifecycle.rs`: a live handoff calls `embed::before_handoff` once its panes are listed and before
  it marks itself in progress; `rollback_handoff_before_commit` calls `embed::handoff_failed`.
- `src/server/handoff.rs`: `spawn_handoff_import` adds `embed::handoff_env()` to the import server's env.
- `src/server/headless.rs`: `extension_host` (the stop `server stop` does, callable in-process, and the outbound
  channel the loop drains into `EndpointControl` messages); a shell client's attach and leave reach the extension, and
  the last one's leave unless the server is stopping or handing off; a client's `jocasta.` message reaches it as
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
  ahead of herdr's own overlays; ticks it with the client's timers; puts its
  status at the left of the tab bar's right side.
- `src/embed.rs` (`QUIT_COMMAND`, and `ShellExtension::docks` marking the boss's workspace) and
  `src/client/shell/{extension,actions,overlay_input,context_menu,tabs,global_menu}.rs`: the boss guard (0665).
  `extension::is_boss_workspace` asks `embed::shell_docks`, since the dock shows beside the boss's workspace only.
  `push_endpoint_method_with_kind` drops, in his client only, a `workspace.close`, `tab.create`, `tab.close`,
  `pane.split` and `pane.close` aimed at it, with the notice "Jocasta owns this workspace: it can't be closed, split
  or given tabs." (`tab.close` too, since closing its only tab closes the workspace). The keys' confirm dialogs and
  prompts check first; the tab-bar `+` is not drawn there; the tab and pane menus drop the refused items; the
  workspace menu does not open. Jocasta's own API calls never pass the client, so they are not affected. The global
  menu's "quit Jocasta" runs the `[[keys.command]]` entry for `jocasta.quit` exactly as its key does (0666).
- `src/client/mod.rs` (the client's failure exit) and `src/lib.rs` (a launch that failed) end the process through
  `embed::exit`, which runs the exit hook first.

### Vendored build dependency

- `vendor/translate-c/` (new): Zig's `translate-c` package (MIT, commit `4e879eb8aba615de112eabd1231ea6e01920cead`,
  17 files as Zig fetched them), and `vendor/libghostty-vt/build.zig.zon` and its `pkg/gtk4-layer-shell`, `pkg/harfbuzz`, `pkg/macos` and `pkg/wuffs` name it by `.path` instead of its
  codeberg.org URL, so a build fetches nothing from codeberg. Its own dependency `aro` still comes from github.com.
- `vendor/libghostty-vt/pkg/simdutf/build.zig`, `pkg/highway/build.zig` and `src/build/SharedDeps.zig` (the `src/simd`
  C++ files) define `_ALLOW_COMPILER_AND_STL_VERSION_MISMATCH` for the MSVC ABI: MSVC 14.52's STL refuses a Clang older
  than its own (STL1000), and Zig 0.16's is Clang 21 (GitHub's `windows-2025-vs2026` image of 2026-11-17 broke the
  Windows build this way; 14.51 on his PC still accepts Clang 20 and up).

## Taking an upstream release

`git subtree pull --prefix=crates/herdr https://github.com/herdrdev/herdr.git vX.Y.Z --squash`, then `git rm` every
file this fork removed that the release changed (a modify/delete conflict), and re-apply the seams listed above.
herdr's own suite (`cargo nextest run -p herdr`) and Jocasta's run before the merge is committed.
