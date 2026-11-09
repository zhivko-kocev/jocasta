# Jocasta

Zhivko's work surface around his daily omp. Three parts (0386, 0392):

- **the agent** is his own [omp](https://github.com/can1357/oh-my-pi), unforked and tuned in its `/settings`; Jocasta adds config, MCP tools and memory only, never code inside omp (0386, 0387);
- **the work surface** is herdr session `jocasta`: workspace "Jocasta" with the boss pane first, then one workspace per project and a worktree workspace per thread (0392, 0412);
- **the companion** is a desktop app (Electrobun on WebView2) with the Jocasta service inside: the rail (the Presence, today, toasts, dictation), whiteboard's canvas, and on the second screen the dashboard's third column (0409, 0428, 0439). It keeps the thread map, reviews finished tasks with hunk (0394), feeds the memory graph and speaks.

## Prerequisites

- Windows 11 (the companion is Windows-only for now).
- [herdr](https://herdr.dev) 0.9.x from its official installer (0385, 0435).
- omp on PATH, signed in (`omp`, then `/login`; 0386).
- Docker Desktop, for the memory graph (Graphiti with FalkorDB; 0334). Without it everything else works and the dashboard says Graphiti is down.

## Install

```powershell
irm https://raw.githubusercontent.com/zhivko-kocev/jocasta/main/scripts/install.ps1 | iex
```

Installs to `%LOCALAPPDATA%\Programs\jocasta\` (or `JOCASTA_INSTALL_DIR`, which must be an empty folder or an earlier Jocasta install; 0049): `jocasta.exe`, `config\` (the boss manual, omp, herdr and Ponytail config), `app\` (the companion), `node\` and `whiteboard\` (whiteboard's server), `hunk\` and `install.json`, and adds the folder to your user PATH. The download's SHA-256 is checked against the hash the release workflow commits in this repository at the tag `checksums-<release tag>`, never against a file from the release, so a replaced release asset is refused (0484). To upgrade, run it again: a running companion is stopped, the new build is copied in, and the companion starts again on it (0440).

To install a local build instead, point `JOCASTA_INSTALL_FROM` at a folder holding `jocasta-windows-x64.zip` and its `.sha256` (`bun run build` writes both to `dist\`; 0487):

```powershell
$env:JOCASTA_INSTALL_FROM = "$PWD\dist"; .\scripts\install.ps1   # from the repo, after `bun run build`
```

`scripts/install.sh` takes `JOCASTA_INSTALL_FROM` the same way, with `jocasta-<target>.tar.gz`.

## First run

```powershell
jocasta setup
```

Setup's changes are numbered migrations (0466). Each one shows its diff and is written only after `y`; it runs once per shipped version, with a marker in `~/.jocasta/agent/setup/`, so a file you edit afterwards is left alone until Jocasta ships something new for it. The first failure stops the rest and says why (exit 1). When changes are waiting, the companion shows one toast at start: "jocasta setup has N change(s) waiting: run `jocasta setup` to see them."

- omp's global MCP servers `jocasta` (`graph_search`) and `whiteboard` in `~/.omp/agent/mcp.json`, and the boss's `jocasta mcp --boss` in the boss's own folder (0389, 0392, 0393);
- omp's settings `~/.omp/agent/config.yml`: a missing file gets `config/omp-config.yml` as it is; an existing one is merged key by key: missing keys are added, keys still at Jocasta's previous value follow the new one, and keys you set differently are kept and shown ("kept: <key>: yours X kept (Jocasta ships Y)"). Comments in your file are lost, as when omp saves its settings (0396, 0420, 0466);
- herdr's omp integration (`herdr integration install omp`), which reports each pane's state to herdr (0388);
- `~/.jocasta/agent/herdr.toml` for session `jocasta` (0401, 0440, 0466);
- `~/.jocasta/agent/jocasta.yaml`, with the `frontier` and `pollers` settings moved from an older Jocasta's settings file when there is one (0387);
- the Windows Terminal scheme Tokyo Night, from the companion's palette (0401, 0474);
- with mise on PATH, mise's machine file `~/.config/mise/conf.d/jocasta.toml` (pinned herdr, hunk and gh; your own `config.toml` wins); with or without mise, the mise skill in `~/.omp/agent/skills/mise/` (0476);
- the calendar's secret address from `JOCASTA_CALENDAR_ICAL_SECRET` into Windows Credential Manager (0430).

To put one file back to the shipped version, run `jocasta setup --refresh herdr.toml`, `jocasta.json` (the Terminal scheme) or `mcp.json` (Jocasta's MCP entries), by name or full path: it shows the diff, asks "Refresh? [y/N]", keeps the old file as `<file>.bak.<YYYYMMDD-HHMMSS>` beside it, and reloads herdr's config after a herdr.toml refresh (0467).

After upgrading, run `jocasta setup` once: since 0484 the boss's MCP entry gets `timeout: 300000`, so a boss call can wait the 2 minutes a confirmation takes (omp gives up after 30 s otherwise).

## Daily use

Run `jocasta` in a terminal. It starts the companion when it isn't running and attaches herdr's client to session `jocasta`; the boss pane runs omp with the boss manual and continues where it was (0424). Ask the boss for a thread in a project; it opens a worktree workspace, plans first, and you approve the plan in its pane. When a task ends, hunk opens beside it with the reviewer's notes (0394); `thread_finish` merges it or opens a PR (0426).

Some boss actions wait for your Allow (0484): merging a thread or opening its PR (`thread_finish`), removing a leftover worktree (`worktree_remove`), starting a thread in a project other than the one whose pane you last focused, and saving a lesson (the window shows its full text). The companion asks in a small window of its own titled "Jocasta: Allow or Deny" (0519), centred and on top of your other windows even while the companion is in the tray, with Allow and Deny (Deny is the default); it leaves the keyboard with the app you're in, so click it, or click into it to use Enter and Escape (0524). Deny, no answer within 2 minutes, closing Jocasta, or no companion running all count as Deny, and the boss is told which. Only the boss pane can use the boss's tools: it gets a per-run key. `graph_search` answers from the pane's own project, each fact quoted «…» with its source and date (0484).

Closing a thread or stopping everything ends every process its panes started (omp's workers, language servers, MCP servers), and a worktree is removed only once no process works in it; otherwise the reason names them (0486).

Jocasta has one life (0434): closing the terminal tab, quitting herdr's client or quitting the companion (the window's × or the tray's Quit) closes everything Jocasta runs: the companion and its whiteboard server, herdr session `jocasta` and the Graphiti and FalkorDB containers (data kept). With threads working or awaiting you, the companion asks first, in its own window titled "Jocasta: Stop everything?": "Stop 2 working threads and 1 awaiting you? Their sessions are saved." (0438, 0519); Keep running in background leaves everything up and hides the window to the tray, and the companion keeps running while the question waits. If herdr's server itself dies, that isn't a close: the companion starts it again and `jocasta` attaches again in the same terminal, saying so meanwhile (0518). An omp that ends in an open Jocasta pane is started again there (0521). Your other herdr sessions and omp windows are never touched. The next `jocasta` continues the boss and every thread.

Hold Right Ctrl anywhere to talk to the boss (0368); the speech model downloads after you say yes. The voice reads the boss's answers when it is on (0418).

## Commands

| Command | What it does |
|---|---|
| `jocasta` | opens the work surface (herdr session `jocasta`) and starts the companion |
| `jocasta setup` | shows what Jocasta adds to omp, herdr, Windows Terminal and mise; writes after `y` (0466) |
| `jocasta setup --refresh <file>` | puts `herdr.toml`, `jocasta.json` or `mcp.json` back to the shipped version after a diff and `y`, keeping a `.bak.<stamp>` copy; an unknown file exits 2 (0467) |
| `jocasta uninstall` | removes everything install and setup added (herdr session `jocasta`, setup's writes, the calendar credential, the `config.jocasta-*.override.toml` mise files, the autostart value, the PATH entry, the app's WebView2 data, `~/.jocasta/agent` except `private-names.txt`, the install folder) after one list and one `y`; asks separately about the data folder and about the containers, images and volumes; keeps your other MCP servers, your `config.yml` keys, your mise `config.toml` and mise itself; refuses while Jocasta is running (0468). It deletes a folder only while it still holds what put it on the list, never follows links or junctions, and never touches a drive root or your home folder ("kept <path>: …"; 0484) |
| `jocasta diagnose` | writes `<data>\diagnose\` (state, the newest 400 log lines) and opens a read-only omp pane, tab "diagnose" in workspace Jocasta, to find what's wrong (0472). The state and log, with project names and folders, go to your model provider with its first message (0484) |
| `jocasta mcp [--boss]` | the MCP server omp starts: `graph_search`, and the boss's tools with `--boss` |
| `jocasta project list \| add <path> \| remove <name\|id>` | lists, adds or removes a project (0111) |
| `jocasta --version`, `-v` | `jocasta <version> (omp <version>, herdr <version>, bun <version>)` (0052) |
| `jocasta --help`, `-h` | the usage |

## Config

- omp's own config stays in `~/.omp/agent` (0387). `config/omp-config.yml` is the reference for your `/settings` pass (0420, 0451): besides the earlier settings it sets `disabledProviders: [claude-plugins]`, `modelRoles.web: web/duckduckgo`, `find.enabled: "off"`, `startup.setupWizard: false`, `startup.changelogMode: hidden`, `github.enabled: true`, `collab.autoStart: "off"`, `secrets.enabled: true` and compaction at 300k tokens (0448).
- Outside `config.yml`, written by `jocasta setup` as a diff after your `y` (0451, 0479): `~/.omp/agent/keybindings.yml` gets `app.live.toggle: []` (Ctrl+L no longer starts omp's live voice), `~/.omp/agent/lsp.json` gets `"idleTimeoutMs": 300000` (your other keys kept in both, and your own value for Jocasta's key kept and shown); `~/.omp/agent/WATCHDOG.md` (the advisor's brief) comes from `config/WATCHDOG.md` when missing, and one you changed is shown and kept, never overwritten. `jocasta uninstall` takes out only the keys setup added, and `WATCHDOG.md` only while it is still Jocasta's text. By hand: your own `~/.omp/agent/secrets.yml`, never in a repo.
- Each project and thread pane also gets `config/guard.yml` (approvals: `bash.gitGuard`, 75 deny rules, then 49 prompt rules; 0450, 0476, 0485) and the project's overlay `<data>/projects/<pid>/omp.yml` (its own memory file, compaction, `AGENTS.local.md`, the lessons skill, and the privacy settings `dev.autoqa: false`, `disabledProviders: [claude-plugins]`, `modelRoles.web: web/duckduckgo` whatever your global config says; 0447–0449, 0459, 0478; the reviewer runs on your `slow` role, 0488); the boss's `config/boss.yml` sets the same privacy settings. What the guard doesn't stop is in its own section below.
- Private project notes go in `AGENTS.local.md` at the project root, gitignored; every pane reads it (0449).
- Jocasta's files are in `~/.jocasta/agent/` (`JOCASTA_CODING_AGENT_DIR` overrides it): `jocasta.yaml` (settings), `projects.yaml` (watched roots), `herdr.toml` (written by setup), `setup/` (setup's markers) and `compose.yaml` (written by Jocasta for the containers).
- `jocasta.yaml` keys: `frontier.port` (default 47400), `frontier.autostart` (on unless `false`), `frontier.quietHours`, `frontier.dashboard.display` (`secondary` or `primary`), `dashboard.weather` (a place or `latitude, longitude`), `pollers.github.everyMinutes`, `pollers.azureDevOps.orgs|everyMinutes`, `pollers.calendar.everyMinutes`, `pollers.news.everyMinutes` and `pollers.news.feeds` (brutalist.report lists; by default every Hacker News and LWN link and the newest 10 gaming; 0442), `graph.main` and `graph.small` (an omp `provider/id` for the graph's model calls; omp's roles by default).
- mise (0473, 0476, 0477): with mise installed, each project can have a Jocasta-kept config `~/.config/mise/config.jocasta-<project id>.override.toml`, never committed; every pane of that project gets `MISE_ENV=jocasta-<project id>`. When a project has none, the boss offers a setup thread that writes it with `test` and `verify` tasks. A repo's own `mise.toml` is used only when trusted under mise's paranoid mode; Jocasta never runs `mise trust`.
- Data: `%LOCALAPPDATA%\jocasta\data` (`JOCASTA_DATA_DIR` overrides it); the companion's log is `<data>\logs\companion-YYYY-MM-DD.log` (7 days kept; 0431).

## What the guard doesn't stop

The guard is a list of command shapes that omp refuses or asks about before running (0450, 0485). It stops the usual ways of losing work or a login: force pushes, deleted remote and force-deleted local branches and refs, `reset --hard`, `clean`, discarding checkouts, dropped stashes (asked first), repo and release deletion through `gh`, recursive deletes, piping downloads into a shell, mise's trust and global changes, git aliases, and reading or printing tokens and credential files (`gh auth token`, `gh auth status -t`, `az account get-access-token`, omp's `auth.json`, gh's `hosts.yml`, SSH keys, Jocasta's `frontier.json`). It also catches `git.exe` and a full path to git, and PowerShell's `rm -Recurse` and `ri` forms.

It is pattern matching, not a boundary. Every pane runs omp as you, with your logins, under omp's `yolo` mode. So the guard can't stop:

- a script the agent writes first and then runs, or an interpreter one-liner beyond the shapes listed in `config/guard.yml` (`python -c`, `node -e`, `bun -e` deletes are asked about; anything else runs);
- shell commands run through omp's `eval` tool (Python or JavaScript), which skip `bash.patterns` entirely;
- a dangerous command hidden in a way the shapes don't match (an unusual flag order, a variable holding the path, a command read from a file);
- anything that reaches your logins or other projects through a tool rather than a command.

It guards the agent's commands only: a command you run yourself in a pane with omp's `!` runs as typed, without the guard (seen in the campaign's re-test, 0481).

A thread that reads a prompt-injected README, test output or web page can therefore still do what you can do. Until thread panes run under an OS boundary (a restricted token, a separate user or a container, without your logins and with limited network; after v1, 0485), treat the guard as a seatbelt and plan approval as the real gate: read the plan before approving it, and keep client repos' own hooks and `.env` files in mind.

The companion's page allows eval (`'unsafe-eval'`) so whiteboard's software maps can route their lines with libavoid (0493); inline scripts and other origins stay blocked. Content that reaches the canvas (a diagram or review an agent draws) is therefore easier to turn into running code than it was; rebuilding libavoid without eval is open for after v1.

## `/share`

`/share` is omp's. By default it uploads the session as an encrypted blob to `my.omp.sh/s`; with `--gist` it makes a GitHub gist. Either way the whole conversation leaves the machine, client code included (0047). Think before you run it.

## Linux and macOS

Builds exist for linux-x64 and darwin-arm64 (`scripts/install.sh`, installing to `~/.local/share/jocasta/`), with no companion: `jocasta` opens herdr without it. They are untested.

## Where things are

- `docs/plan.md`: the final plan. The vision in its section 1 is the source of truth (0019).
- `docs/decisions/`: the decision log and feature register.
- `AGENTS.md`: the rules every agent follows here.

## Development

Requires Bun 1.4.3 (0036, 0042, 0435). With [mise](https://mise.jdx.dev) the repo's `mise.toml` pins Bun 1.4.3 and Node 24.21.0 (`mise.lock` holds their URLs and checksums) and loads without trust (0476):

```powershell
mise install --locked
mise run check                       # typecheck, lint, the app's tsc and the tests
mise run build                       # bun run build
mise run evals                       # bun run evals
mise run smoke                       # the herdr, Graphiti and dictation smokes on this machine
```

Or with Bun directly:

```powershell
bun install
bun run typecheck                    # tsc --noEmit (0037)
bun run lint                         # biome check
bun run format                       # biome check --write
bun test                             # page tests need Microsoft Edge; they skip without it
bun src/entry.ts                     # a dev run of the CLI
bun run build --target windows-x64   # also linux-x64, darwin-arm64 (0043)
```

Opt-in smokes: `JOCASTA_HERDR_SMOKE=1` (a real herdr), `JOCASTA_GRAPHITI_SMOKE=1` (Docker), `JOCASTA_OMP_GATEWAY_SMOKE=1` (a model call through your omp's `auth-gateway stdio`, 0463), `JOCASTA_DICTATION_SMOKE=1` (the real dictation model, 0440). Live checks set `JOCASTA_HERDR_SESSION` to a throwaway session name so they never touch session `jocasta` (0441). Evals run omp on real cases: `JOCASTA_EVALS_REAL=1 bun run evals` (0375).

`bun run build` writes `dist/<target>/` (`jocasta.exe`, `config/`, `node/`, `whiteboard/`, `hunk/`, and on Windows `app/`) plus `dist/jocasta-<target>.zip|.tar.gz` and its `.sha256` (0044). whiteboard is built from the clone in `forks/whiteboard` at v0.2.4 (0417). On Windows the build also makes the companion (0367) into `dist/windows-x64/app/`: its main process is `src/app/main.ts`, built by Hutch 0.27.1 for the exact `electrobun` 2.0.2 devDependency; on first use it downloads Hutch, Electrobun's runtime (with its own Bun 1.4.0, which the app runs on) and its toolchains into `node_modules/.cache/hutch` (override with `HUTCH_HOME`), and never touches PATH. `JOCASTA_APP_IDENTIFIER` gives a test build its own identifier, so it shares neither the installed app's WebView2 data nor its `Jocasta` autostart value. After a build, `bunx tsc -p src/app` type-checks the app's main process against Electrobun's SDK. Pushing a `v*` tag publishes a GitHub release (0051).

## Credits

- [herdr](https://github.com/herdrdev/herdr) (Apache-2.0): the work surface; its Tokyo Night is the companion's palette.
- [oh-my-pi](https://github.com/can1357/oh-my-pi) (MIT): the agent in every pane.
- [whiteboard](https://github.com/devdotfast/whiteboard) (MIT): the canvas and its server, shipped with its LICENSE (0393).
- [hunk](https://github.com/modem-dev/hunk) (MIT): the review pane (0394).
- [Electrobun](https://github.com/blackboardsh/electrobun) (MIT): the companion's shell (0367).
- [Monaco Editor](https://github.com/microsoft/monaco-editor) (MIT): the canvas's code peeks and diffs (0411).
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (Apache-2.0) with the [Kokoro](https://huggingface.co/hexgrad/Kokoro-82M) voice and NVIDIA's Parakeet TDT 0.6B v3 for dictation (0330, 0418).
- [Graphiti](https://github.com/getzep/graphiti) (Apache-2.0) with [FalkorDB](https://github.com/FalkorDB/FalkorDB), in containers (0334).
- [Node.js](https://nodejs.org) (MIT), shipped with its LICENSE to run whiteboard's server.
- [Ponytail](https://github.com/DietrichGebert/ponytail) (MIT), in `config/ponytail.md` (0421).
- [Mona Sans](https://github.com/github/mona-sans) by GitHub and [Geist Mono](https://github.com/vercel/geist-font) by Vercel, both under the SIL Open Font License 1.1; the font files and their licences ship in `src/frontier/ui/fonts/` (0369).
- News headlines from [brutalist.report](https://brutalist.report) (0429); weather from [Open-Meteo](https://open-meteo.com) (CC BY 4.0).
