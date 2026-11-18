# Jocasta as two plugins: the facts (2026-11-18)

His idea of 2026-11-18: Jocasta ships as two parts, a herdr plugin and an omp plugin carrying the tuned settings, skills, hooks, the MCP entry and the boss manual, in place of today's installer, `jocasta setup` and `jocasta uninstall`. This page holds the facts measured that day, for the grilling that decides it. Nothing here is decided.

## What runs where today

- **The installer** (`scripts/install.ps1`, `install.sh`): downloads the release, checks its SHA-256 against the `checksums-<tag>` tag (0484), refuses a folder holding files it doesn't own (0579), swaps the install folder (on Windows renaming in-use files first), and puts the folder on PATH. It doesn't run setup.
- **`jocasta setup`**: 17 numbered migrations, shown as one diff and applied after one `y`, each with a marker and an undo record under `~/.jocasta/agent/setup/`. Ten of them write into omp's own folder (`mcp.json`, `config.yml`, `keybindings.yml`, `lsp.json`, `WATCHDOG.md`, `secrets.yml`, the mise skill, herdr's omp extension through `herdr integration install omp`); two into herdr (Jocasta's own `herdr.toml` and the plugin link); the rest into mise's `conf.d`, Windows Terminal's fragments, the registry, or clean-up of earlier versions (Graphiti's containers, the companion's Run value, whiteboard in the user `mcp.json`).
- **At run time, outside setup**: the boss's files (`APPEND_SYSTEM.md` from `config/boss-manual.md`, `config/boss-skills/`, the lessons skill) are copied into `<data>/home/.omp/` on every start; panes get `config/guard.yml`, the project overlay, `heavy.yml` and `boss.yml` as `--config` files, `ponytail.md` as `--append-system-prompt`, and `pane-check-extension.ts` as `-e`, all by path beside the binary.
- **`jocasta uninstall`**: undoes what setup wrote while it is still setup's text, unlinks the herdr plugin, removes the PATH entry, the install folder's own files and, when asked, the data.

## omp 18.22.0's plugins (`omp plugin --help`; omp's `marketplace.md`, `plugin-manager-installer-plumbing.md`, `extensions.md`, `config-usage.md`)

- Three ways in: an npm or git package (`omp plugin install github:owner/repo`, through Bun inside omp; the package's root `package.json` carries an `omp` manifest), a local link (`omp plugin link <dir>`, a junction on Windows), or a marketplace: a repo with `.omp-plugin/marketplace.json` whose entries may point at a subfolder (`git-subdir`) pinned by `ref` and `sha`; `omp plugin marketplace add owner/repo`, then `omp plugin install name@marketplace`, `--scope user|project`, `upgrade`, `uninstall`, `enable`, `disable`.
- A plugin folder can carry skills (`skills/<name>/SKILL.md`), agents (`agents/`), slash commands, rules, prompts, hooks, tools, MCP servers (`mcp.json` / `.mcp.json`), LSP servers, DAP adapters, and extension modules (`package.json` `omp.extensions`). Extensions are TypeScript run inside omp; install checks that each one loads.
- A plugin cannot ship omp settings as a file. An extension can set them at run time: `lookup(id).override(pi.pi.settings, value)` writes an in-memory layer above project, global and `--config` layers, never saved; `isConfigured()` lets it leave a value he set himself alone.
- MCP servers from plugins don't show in `/mcp list` when filtered out; only the two primary `mcp.json` files do.
- Installed plugins are trusted code in omp's process, no sandbox. Newly installed hooks and extensions need a new session.

## herdr 0.9.5's plugins (`herdr plugin --help`, `herdr plugin install --help`; herdr.dev's plugin docs)

- Install from GitHub only (`herdr plugin install owner/repo[/subdir] --ref <ref>`), or link a local folder; `update`, `uninstall`, `enable`, `disable`, `config-dir`, `log`.
- A manifest declares `[[build]]` (runs on his machine at install; herdr installs no toolchain), one-shot `[[startup]]` commands, `[[actions]]`, `[[events]]`, `[[panes]]` (overlay, popup, split, tab, zoomed) and `[[link_handlers]]`. There's no download of a release binary.
- Startup commands are one-shot, "not supervised daemons"; herdr doesn't keep a background process alive or stop it.
- A plugin can't bind keys or set herdr's config. Jocasta's keys and settings (`Ctrl+B` then `Alt+F/S/R/N/Q`, the HUD, the theme, `resume_agents_on_restore`) stay in a config file someone writes; today that is Jocasta's own `~/.jocasta/agent/herdr.toml`, read through `HERDR_CONFIG_PATH`.

## What follows from the facts

- **The service stays a Rust process**: neither plugin system runs a long-lived program. The thread map, the Allow and push gates, the relay, the pollers, the ledger and the history index keep running in `jocasta`, started as today.
- **The binary still has to arrive**: herdr can only build from source on his machine, and omp's packages are TypeScript. The installer (or a package manager) still delivers `jocasta`, and both plugins call it on PATH.
- **What could move into an omp plugin**: the `jocasta` MCP entry, the mise skill, the drift agent, `pane-check-extension.ts`, and the tuned settings as an extension that overrides only values he hasn't set. What can't: the boss's own folder (its manual, skills and `jocasta mcp --boss` live in the boss's home), and the overlays chosen per pane (guard, project, heavy, boss), which are `--config` files by design.
- **What could move into a herdr plugin installed from GitHub**: the manifest Jocasta links today. The keys and herdr settings stay in Jocasta's `herdr.toml`.
- **What no plugin does**: PATH, the checksum and folder checks, Windows Terminal's scheme, mise's `conf.d` fragment, the Windows folder fence in his `config.yml`, the clean-up of earlier versions.
- **omp on his PC is 18.22.0**; the pinned version is 18.20.2 (0623), and the shipped config's comments name 18.20.2's source.

## His answers so far (2026-11-18, the grilling under way)

- **Status:** v1, built before the VM runs.
- **Shape, in his words:** "we publish herdr plugin here https://herdr.dev/tui/plugins/ that is one part of jocasta and jocasta instead of reinventing or proxying just using herds cli … maybe we can remove duplicate code, and the second part is basically a snapshot of the most optimal state of omp (fine tuned settings, global skills, hooks …)".
- **omp's version:** "always latest", in place of a pin.

herdr's marketplace (herdr.dev/tui/docs/marketplace/, read that day) indexes public GitHub repos with the `herdr-plugin` topic and a parseable `herdr-plugin.toml` on the default branch, at the root or in subfolders, refreshed every 30 minutes; there is no review and no submission. herdr's docs call the whole herdr CLI the plugin API, reached through `HERDR_BIN_PATH`.

Later the same session:

- **CI's audit:** ignore picomatch's GHSA by ID (record 0648, built).
- **Where Jocasta lives:** inside his everyday terminal, not a separate session: "i would love for it to be performant useful and optimal first".
- **The binary:** asked whether Jocasta needs its own binary if it uses herdr's CLI, then: "can we just fork herdr and adapt our vision there?" Shown the costs (herdr v0.9.5: about 11.8 MB of Rust, its core about 4.6 MB, its client UI about 3.5 MB, libghostty-vt vendored in Zig at 19.6 MB; releases every one to two weeks; upstream takes no outside pull requests), he chose between a full fork and the core only: "full fork trimmed, i love it, we take all that we use or can be useful in future for jocasta and drop other things and make herdr lightweight also".
- **Zig:** approved through mise, pinned to the version herdr's vendored build names.
- **The plan wait and the 5 s tick, and the keys:** left until the fork was decided; inside the fork both become in-process calls and Jocasta's own config.
- **The omp part:** not an omp plugin: "just keeping the files basically like a snapshot, these are the most optimal settings already set up, the global skills needed and stuff like that — to keep the lightweight omp offers but still to make it perform better and cheaper"; "it doesnt have to be a plugin". How it is applied: below.
  - Fact for that question (omp 18.22.0's `cli-reference.md`, `config-usage.md`): `omp --profile <name>` (or `OMP_PROFILE`) moves the whole user base to `~/.omp/profiles/<name>/agent/`: settings, skills, MCP, sessions, caches and logins are its own, so a `jocasta` profile would need its own sign-in and keep its sessions apart from his everyday ones; only keybindings are inherited from the default profile.
- **omp's version, again:** "for the jocasta we will have pinned, for development until everything ready we use latest". His PC runs omp 18.22.0; the pin is 18.20.2 (0623), so the release's pin moves when everything is ready, with 5.4's seams re-checked.
- **The trim (2026-11-18):** the core in the table below kept; the always-dropped list dropped; of the open parts, **kitty graphics kept** and the **plugin system kept** ("if we can use some extensions"; the marketplace's plugins checked that day, among them a file viewer, a diff reviewer, Neovim, a browser and phone clients), so the CLI and socket stay compatible with upstream's for plugins; **live handoff kept**, with Jocasta's own upgrade using it on Linux and macOS ("jocasta should be macos, linux and windows").
- **Where it lives:** in this repo ("the herdr fork + omp optimal snapshot is jocasta, is this repo, they should be here"): brought in with `git subtree` at `crates/herdr`, upstream history kept, the repo a Cargo workspace.
- **Upstream:** from v0.9.5; each upstream release read and merged deliberately, its changes to kept parts taken, with the fork's tests and Jocasta's suite run.
- **Processes:** one background server: herdr's server and Jocasta's service are one ("if we can reuse something from herdr background server then we dont need the jocasta service part"); the terminal is its client; the socket API stays for omp panes, plugins and the CLI; `today` and questions drawn natively.
- **The command:** one binary, `jocasta`, with herdr's verbs joining Jocasta's at the top level and `HERDR_BIN_PATH` pointing at it; `worktree` one merged command.
- **Files and env:** everything under Jocasta's own dirs so a plain herdr can sit beside it; panes keep herdr's `HERDR_*` names ("jocasta and herdr are one now, we are doing an upgraded fork like omp did to pi").
- **The omp snapshot's application:** replace: setup backs up `~/.omp/agent`'s files the snapshot has and writes the snapshot over them (chosen over a merge and over an omp profile).
  - Scope: every file the snapshot has is replaced, `mcp.json` whole too (his other servers kept in the backup); `secrets.yml` only when missing; logins, sessions, memory and history never touched; a diff and his `y` first, as setup asks today.
- **The way to v1 (his words, 2026-11-18):** "we do a prewalk with fable 5.2 xhigh to opus 5.5 high for this new approach to review and expand it for the forks side, and for the config you try every value and see the most performant state of omp, the most optimal state, best performance for the optimal price, and also watching the pc from not running out of memory. one full run until v1 with all the functionalities jocasta had + improvements along the way, speedup, deduplication, optimization, pruning … use ponytail if needed or some other skills to pull this off".

## The fork: herdr 0.9.5's parts, measured (2026-11-18, for the trim question)

Source: herdr's v0.9.5 tree (cloned read-only to `D:/jocasta-spikes/herdr-0.9.5`) and GitHub's tree listing; sizes are bytes of `.rs`. herdr is one Cargo package with a `ghostty-vt` crate (its vendored libghostty-vt is Zig 0.16.0, pinned at commit `44f2a44`) and a patched `portable-pty`; its own toolchain pin is Rust 1.96.1. Server and client are one binary: `main` dispatches to `server::headless::run_server()` and `client::run_client()`. `run_server` reads argv, sets process-wide state and exits the process on a busy socket, so running it inside Jocasta needs a `run_server_with(config, options)` split [INFERENCE].

| Part | Size | Jocasta uses it | Proposal |
|---|---|---|---|
| Server, workspaces, tabs, panes, layout, popups | `src/server` 1.1 MB, `workspace*`, `layout.rs`, `pane.rs` | every pane, the `today` split, the `ask` popup | keep |
| pty and the patched portable-pty, platform layer | `src/pty` 0.11 MB, `vendor/portable-pty`, `src/platform` 0.55 MB | every pane; Windows first | keep |
| Terminal emulation (libghostty-vt), scrollback, selection, copy mode, mouse, clipboard | `crates/ghostty-vt` 0.7 MB + Zig, `src/terminal`, `src/pane` | every pane; copying from a pane | keep |
| Client UI (ratatui), input, keybindings, themes, sidebar, tab bar, window title | `src/client` 2.4 MB, `src/app` 1.4 MB, `src/ui`, `src/input` | the surface he sees; keys, HUD, theme | keep, and draw `today` and questions natively |
| Socket API, CLI, events | `src/api`, `src/cli`, `src/protocol` | ~30 methods, `events.subscribe` | keep the API (the boss and threads, `jocasta` CLI); in-process calls inside |
| Persistence and restore | `src/persist`, `session.rs` | `pane_history`, restore after a crash | keep |
| Agent detection | `src/detect` 0.18 MB, 22 manifest files | omp's state | keep omp's (and pi's) manifest, drop the rest |
| Integrations | `src/integration` 0.42 MB, hooks for 18 agents (antigravity, claude, codex, copilot, cursor, devin, droid, grok, hermes, kilo, kimi, letta, mastracode, omp, opencode, pi, qodercli, qwen) | `herdr integration install omp` | keep omp's (and pi's, omp's base), drop the other 16 |
| (the benchmark) | `bench/lib/launch.rs` | `jocasta-bench` starts omp, Claude Code and opencode in panes of its own herdr session and waits on `pane.wait_for_output`, not on agent detection | dropping the other agents' manifests and hooks leaves the benchmark working [INFERENCE from the code] |
| Worktrees, git status | `worktree.rs`, `workspace/git`, `app/git_refresh.rs` | `worktree.create`, sidebar branch rows | keep |
| Notifications, sound | `server/notifications.rs`, `sound.rs` | `notification.show`, request sound | keep |
| Plugins | `plugin_*`, `cli/plugins.rs`, `persist/plugin_registry.rs` | today's `jocasta` plugin | open: native parts replace Jocasta's plugin; keep for others' plugins, or drop |
| Kitty graphics | `kitty_graphics*` 0.12 MB, `png` | none | open: drop, or keep for images in panes later |
| Remote SSH attach, machine API | `src/remote` 0.24 MB, `cli/machine.rs` | none | drop |
| Live handoff (Unix) | `handoff_runtime.rs`, `server/handoff*` | none (upgrades restart the server) | open |
| Update checker, manifest updates, release notes, onboarding, product announcements | `update.rs` 0.14 MB, `detect/manifest_update.rs`, `release_notes.rs`, `product_announcements.rs`, `ui/onboarding.rs` | off in Jocasta's config | drop |
| Render-scale benchmark, capture scripts | `server/render_scale_benchmark.rs`, `scripts/capture_*` | none | drop |
| Packaging and docs | `nix/`, `flake.*`, `distribution/`, `docs/next/`, `README.zh-CN.md`, release scripts | none (Jocasta has its own installer) | drop |

Tests: 33 integration test files in `tests/`, most of them spawning the real binary; `remote_attach`, `machine_api`, `machine_setup` and `live_handoff` go with their parts. Sizes of the drop and open candidates (`.rs` bytes): plugins 354 KB, update checker 172 KB, remote 250 KB, kitty graphics 125 KB, manifest updates 34 KB, machine API 30 KB, render-scale benchmark 24 KB, live handoff 23 KB (Unix only: `#[cfg(unix)]`), release notes 20 KB, product announcements 11 KB.
