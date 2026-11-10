# Jocasta

Zhivko's work surface around his daily omp. Three parts (0386, 0392, 0552):

- **the agent** is his own [omp](https://github.com/can1357/oh-my-pi), unforked and tuned in its `/settings`; Jocasta adds config, MCP tools and memory only, never code inside omp (0386, 0387);
- **the work surface** is herdr session `jocasta`: workspace "Jocasta" with the boss pane first and the `today` pane beside it, then one workspace per project and a worktree workspace per thread (0392, 0412, 0553);
- **the Jocasta service** is a plain Bun process (`jocasta service`) that `jocasta` starts beside herdr, with the `jocasta` herdr plugin it drives: the `today` pane, the `ask` popup for its questions, and keys for Finish, Stop, Review, New thread and Quit (0551–0554). It keeps the thread map, reviews finished tasks with hunk (0394), runs the threads' checks and the code index (0542), feeds the memory graph, and opens whiteboard's diagrams and reviews in your browser (0393, 0552).

## Prerequisites

- Windows 11 (Linux and macOS builds exist; the test suite passes there, the builds themselves are untested; see below).
- [herdr](https://herdr.dev) 0.9.5 or a later 0.9.x, from its official installer (0385, 0435); the plugin needs 0.9.5 (0553).
- omp on PATH, signed in (`omp`, then `/login`; 0386). This build is checked against omp 18.18.6 (0535, 0589). Dictation is omp's own: omp downloads its speech model on first use or with `omp setup speech` (0556).
- Docker Desktop, for the memory graph (Graphiti with FalkorDB; 0334). Without it everything else works. On Linux it must be Docker Desktop: with the native Docker Engine the Graphiti container can't reach Jocasta's model endpoint, so there is no memory there, and `today` shows "Graphiti down (needs Docker Desktop on Linux: the container can't reach Jocasta's model endpoint)" (0584).
- [mise](https://mise.jdx.dev), optional: with it, setup pins herdr, hunk, gh and codebase-memory-mcp 0.12.1, the code index behind `graph_search` (0476, 0542, 0562); without mise there is no code index.

## Install

```powershell
irm https://raw.githubusercontent.com/zhivko-kocev/jocasta/main/scripts/install.ps1 | iex
```

Installs to `%LOCALAPPDATA%\Programs\jocasta\` (or `JOCASTA_INSTALL_DIR`; 0049): `jocasta.exe`, `config\` (the boss manual, omp, herdr, the herdr plugin and Ponytail config), `node\` and `whiteboard\` (whiteboard's server), `whiteboard-canvas\` (its canvas), `editor.worker.js` (Monaco's worker), `hunk\` and `install.json`, and adds the folder to your user PATH. The folder is Jocasta's alone (0579): it must be empty or hold an earlier install whose `install.json` names that same folder; any other folder is refused ("jocasta install: <folder> holds files that aren't Jocasta's; choose an empty folder for JOCASTA_INSTALL_DIR."). `install.json` lists the release's top-level entries, which are what `jocasta uninstall` removes. The download's SHA-256 is checked against the hash the release workflow commits in this repository at the tag `checksums-<release tag>`, never against a file from the release, so a replaced release asset is refused (0484). The release is unpacked with Windows' own `tar` beside the install and checked before the install is touched. To upgrade, run it again: the Jocasta service and whiteboard's server stop (and the companion app an older install left running: "jocasta install: stopping the companion app an earlier install left running."), herdr's panes keep running, files still in use are renamed aside as `<name>.old-<stamp>` and the new build is copied in; the next `jocasta` starts the service on it (0552). omp sessions and hunk panes started before the upgrade run the old build until they restart.

To install a local build instead, point `JOCASTA_INSTALL_FROM` at a folder holding `jocasta-windows-x64.zip` and its `.sha256` (`bun run build` writes both to `dist\`; 0487):

```powershell
$env:JOCASTA_INSTALL_FROM = "$PWD\dist"; .\scripts\install.ps1   # from the repo, after `bun run build`
```

`scripts/install.sh` takes `JOCASTA_INSTALL_FROM` the same way, with `jocasta-<target>.tar.gz`.

## First run

```powershell
jocasta setup
```

Setup's changes are numbered migrations (0466). Each one shows its diff and is written only after `y`; it runs once per shipped version, with a marker in `~/.jocasta/agent/setup/`, so a file you edit afterwards is left alone until Jocasta ships something new for it. The first failure stops the rest and says why (exit 1). When changes are waiting, the service says so once at start, in a notification through herdr: "jocasta setup has N changes waiting: run `jocasta setup` to see them."

- omp's global MCP server `jocasta` (`graph_search`; `report_result` and `check` in thread panes) and `whiteboard` in `~/.omp/agent/mcp.json`, and the boss's `jocasta mcp --boss` in the boss's own folder (0389, 0392, 0393, 0539, 0542, 0577);
- omp's settings `~/.omp/agent/config.yml`: a missing file gets `config/omp-config.yml` as it is; an existing one is merged key by key: missing keys are added, keys still at Jocasta's previous value follow the new one, and keys you set differently are kept and shown ("kept: <key>: yours X kept (Jocasta ships Y)"). Comments in your file are lost, as when omp saves its settings (0396, 0420, 0466);
- herdr's omp integration (`herdr integration install omp`), which reports each pane's state to herdr (0388);
- `~/.jocasta/agent/herdr.toml` for session `jocasta` (0401, 0440, 0466, 0554);
- `~/.jocasta/agent/jocasta.yaml`, with the `frontier` and `pollers` settings moved from an older Jocasta's settings file when there is one (0387);
- the Windows Terminal scheme Tokyo Night, from Jocasta's palette (0401, 0474);
- with mise on PATH, mise's machine file `~/.config/mise/conf.d/jocasta.toml` (pinned herdr, hunk, gh and codebase-memory-mcp; your own `config.toml` wins); with or without mise, the mise skill in `~/.omp/agent/skills/mise/` (0476, 0562);
- omp's `keybindings.yml`, `lsp.json`, `WATCHDOG.md` and `secrets.yml` (under Config below; 0451, 0479, 0556, 0594);
- the logon Run value an earlier install's app wrote, removed: Jocasta no longer starts at logon; run `jocasta` yourself (0557);
- the `jocasta` herdr plugin, written to `~/.jocasta/agent/herdr-plugin/` with how herdr starts this Jocasta, linked with `herdr plugin link`, and herdr's config reloaded so its keys find the plugin's actions (0553, 0558);
- the calendar's secret address from `JOCASTA_CALENDAR_ICAL_SECRET` into Windows Credential Manager (0430).

To put one file back to the shipped version, run `jocasta setup --refresh herdr.toml`, `jocasta.json` (the Terminal scheme) or `mcp.json` (Jocasta's MCP entries), by name or full path: it shows the diff, asks "Refresh? [y/N]", keeps the old file as `<file>.bak.<YYYYMMDD-HHMMSS>` beside it, and reloads herdr's config after a herdr.toml refresh (0467).

After upgrading, run `jocasta setup` once: this build links the herdr plugin, binds F9 for dictation and removes the autostart value (0553, 0556, 0557), and sets omp's time limit for Jocasta's MCP calls (0577): 31 minutes (`"timeout": 1860000`) on the global `jocasta` server, since a thread's `check` may run 30 minutes, and 35 minutes (`2100000`) on the boss's, since a call may also wait for your Allow (omp gives up after 30 s otherwise). `jocasta mcp` itself never cuts a call for being idle; the service bounds each step. Setup refuses to write a file that changed after it showed the diff ("… changed since the diff; run jocasta setup again"), edits only its own lines in `keybindings.yml`, and reloads herdr after a `herdr.toml` change ("herdr reloaded its config."; 0584).

## Daily use

Run `jocasta` in a terminal. It starts the Jocasta service when none answers and waits for it (a hung one is ended after 15 s, herdr's session and its panes spared, and replaced; 0565, 0582), then attaches herdr's client to session `jocasta`; if the service stops while starting, `jocasta` prints its last error and "jocasta: run `jocasta diagnose` for the full log." and exits 1 (0582). The boss pane runs omp with the boss manual and continues where it was (0424), with `today` beside it (boss 65 / `today` 35; 0553). Ask the boss for a thread in a project, press Ctrl+B then Alt+N (pick the project, write the brief), or run `jocasta thread new`; it opens a worktree workspace, plans first, and you approve the plan in its pane. Each thread runs with `--warn-cost 50` (0541) and can carry `done_when` commands its review runs (0540); it ends a task with `report_result` (0539), and its `check` runs the project's checks once per git tree (0542, 0546). When a task ends, hunk opens beside it with the reviewer's notes (0394); `thread_finish` merges it or opens a PR (0426). The boss's `big_picture_review` answers at once ("Started acme-api's big-picture review; its result comes as an [event].") and its result reaches the boss later as one `[event]` line; one runs per project at a time (0577).

`thread_status` and `jocasta thread list` give each thread's ledger line, such as `spend $1.84 · peak context 41% (82k of 200k) · cache read 71% · 3 re-reads · 1 repeated call · 1 cache miss · 2 compactions` (0546). Requests are every assistant message and every side call omp records (`model_usage`); a cache miss is a cache write of 10,000 tokens or more after more than 5 minutes without a request on the same provider and model; compactions are omp's own, not Jocasta's `/handoff` (0582). "$ today" adds the memory graph's model calls, one line each in `<data>/graph-usage.jsonl` (0582).

`today` shows, top to bottom: the date and time, `quiet` during quiet hours, a service that is down in red with its note, the weather (when `dashboard.weather` is set), CPU and memory, the quota windows (red from 80%, with the reset time when it is within a day: `5h 91% (16:30)`) and today's spend; NEEDS YOU (each row starts with its kind: `?` a question or plan, `✓` reviewed, `↗` a review request, `!` a red pipeline or a full disk; a reminder starts with its time); UNDERWAY (each thread's state, time, cost and context); LANDED TODAY; CALENDAR (a meeting that started shows `now` until it ends); NEXT (your notes) and NEWS while rows are left; and its keys, `↑↓ pick · Enter go/open · f Finish · s Stop · r Review · n New` (0555, 0581). Enter goes to a thread's pane or opens a row's link (such rows end in `↗`); `r` reviews the picked thread and `n` starts a new one. In a narrow pane a row keeps at least 18 columns of its title and drops context, cost and time first; the header drops the weather, then CPU and memory, before it passes two rows; the key line drops its last keys. Until the service first answers the header says "Connecting to Jocasta…"; while it doesn't answer, "Jocasta isn't answering (since 18:03); trying again…", and a key pressed then says "Not connected; try again." A key says what it started at once ("Starting a thread in acme-api: worktree, omp, brief…"); the outcome stays a minute (0581). herdr's tab bar shows `N awaiting you · $X today` (N is NEEDS YOU's rows) and the time (0554, 0581). In any pane, Ctrl+B then Alt+F finishes the focused thread (merge or PR), Alt+S stops its run, Alt+R reviews its commits with hunk beside it, Alt+N starts a new thread and Alt+Q quits Jocasta (0554); Alt+1…9 switch workspaces and Alt+[ / Alt+] move between agents (0408).

Some actions wait for your Allow (0484, 0544): merging a thread or opening its PR (`thread_finish`), removing a leftover worktree (`worktree_remove`), starting a thread in a project other than the one whose pane you last focused or with a `done_when` the guard asks about (0580), saving a lesson (the question shows its full text), and from the command line `jocasta thread send|stop|finish` and `jocasta worktree remove`. The question opens in herdr's `ask` popup, titled "Jocasta: Allow or Deny", in front of whatever you're on (0553). Allow takes the three letters the popup shows: type them; the arrows or Tab pick a choice, and Esc, or Enter on Deny, denies (0558). Finish's Merge and PR and the close question's Stop everything take a code the same way; Cancel stays one key (0568). The popup ignores keys for its first 750 ms, so typing meant for a pane isn't spent on it; the code stays on screen and PgUp/PgDn read a long question; after a second wrong code it says "Wrong code; one try left, then it counts as Deny." (0578). Deny, a third wrong code, no answer within 2 minutes, closing the popup any other way, or closing Jocasta all count as Deny, and the boss is told which. One question per action and thread is open at a time: asking again brings it to the front ("Finish: already asking about acme-api/retry."), and Allow questions go before the ones your keys opened (0578). Only the boss pane can use the boss's tools: it gets a per-run key. Every omp Jocasta starts in a pane gets that pane's own key, `JOCASTA_PANE_KEY`, kept in `<data>/panes.json`; `jocasta mcp` sends it, so a tool call can't claim to come from another pane. A pane started by a build before this one gets its key when its omp starts again (0578). `graph_search` answers from the pane's own project, each fact quoted «…» with its source and date (0484), and relationship questions also get code locations from the code index (0562).

When a thread needs you, herdr shows a system notification with its request sound; a finished thread chimes with herdr's done sound (0554, 0569). Every notification is also a line in `<data>/notifications.jsonl`.

Closing a thread or stopping everything ends every process its panes started (omp's workers, language servers, MCP servers), and a worktree is removed only once no process works in it; otherwise the reason names them (0486). A process counts as a pane's only when it started at or after its parent, so a process whose parent's pid Windows has given out again is never reached; `taskkill /T` isn't used (0573's review, SRV-A-1).

Nothing runs without Jocasta (0549): closing the terminal, quitting herdr's client or Ctrl+B then Alt+Q stops everything Jocasta started: the service, herdr session `jocasta` and every omp in it, the relay, the code index, whiteboard, hunk, and the Graphiti and FalkorDB containers (data kept). There is no keep-running-in-the-background. With threads working or awaiting you, closing the terminal opens a new terminal on the same herdr session (Windows Terminal, else a console window) and asks in the popup, titled "Jocasta: Stop everything?": "Stop 2 working threads and 1 awaiting you? They pick up where they left off next time."; Cancel leaves everything as it was (0563, 0581). The quit key asks the same. On the next `jocasta`, threads that were mid-turn go on with that turn (`omp --restore`) and the others reopen idle; GitHub, Azure DevOps and the calendar are read again, and reminders and events that came due meanwhile show once (0549). If herdr's server itself dies, that isn't a close: the service starts it again and `jocasta` attaches again in the same terminal, telling a healed crash from your quitting by the service's restart count (0518, 0566). The service opens `today` again when it is gone (0553), and an omp that ends in an open Jocasta pane is started again there (0521). Your other herdr sessions and omp windows are never touched.

Hold Right Ctrl anywhere in Windows to talk to the boss: the service sends F9 (omp's `app.stt.toggle`) to the boss pane through herdr on press and again on release, omp transcribes on this machine with its local Parakeet model, and the text is sent when you let go (0556). Holding Space in a focused omp pane is omp's own push-to-talk. There are no spoken replies (0552).

The boss's `show` opens a whiteboard review in your browser, on a page the service serves on 127.0.0.1; a "Reviews" button goes back to the list of reviews (0393, 0552).

## Commands

| Command | What it does |
|---|---|
| `jocasta` | opens the work surface (herdr session `jocasta`) and starts the Jocasta service |
| `jocasta setup` | shows what Jocasta adds to omp, herdr, Windows Terminal and mise; writes after `y` (0466) |
| `jocasta setup --refresh <file>` | puts `herdr.toml`, `jocasta.json` or `mcp.json` back to the shipped version after a diff and `y`, keeping a `.bak.<stamp>` copy; an unknown file exits 2 (0467) |
| `jocasta uninstall` | removes everything install and setup added (herdr session `jocasta`, setup's writes and the herdr plugin's link, the calendar credential, the `config.jocasta-*.override.toml` mise files, the PATH entry, an earlier install's app's autostart value and WebView2 data, `~/.jocasta/agent` except `private-names.txt`, and from the install folder only the entries its `install.json` lists, then the folder once it is empty) after one list and one `y`; asks separately about the data folder and about the containers, images and volumes; keeps your other MCP servers, your `config.yml` keys, your mise `config.toml` and mise itself; refuses while Jocasta is running (0468, 0579). A folder that also holds your files stays ("removed Jocasta's files from <folder>; kept the folder (it holds other files)"); an install from before the list is removed whole only when every entry is a release's name, else it is kept ("kept <folder>: it holds files that aren't Jocasta's (…); remove Jocasta's files from it by hand"). On Windows it first refuses while omp sessions run programs from the install ("jocasta: omp sessions still run programs from <folder>: jocasta.exe mcp (pid 4120), … Close them, then run jocasta uninstall again."), and the files the running `jocasta.exe` holds go once it exits. It deletes a folder only while it still holds what put it on the list, never follows links or junctions, and never touches a drive root or your home folder ("kept <path>: …"; 0484). It works the same on Linux and macOS |
| `jocasta diagnose` | writes `<data>\diagnose\` (state, the newest 400 log lines) and opens a read-only omp pane, tab "diagnose" in workspace Jocasta, to find what's wrong (0472). The state and log, with project names and folders, go to your model provider with its first message (0484) |
| `jocasta mcp [--boss]` | the MCP server omp starts: `graph_search` everywhere, `report_result` and `check` only in thread panes (`JOCASTA_THREAD` set) while the layer is on, and the boss's tools with `--boss` (0577) |
| `jocasta project list \| add <path> \| remove <name\|id>` | lists, adds or removes a project (0111) |
| `jocasta thread new <project> "<brief>" [--title <t>] [--done-when "<cmd>"]... [--heavy]` | starts a thread through the running service, as the boss's `thread_start` does: plan mode, a task file, its `done_when` commands (0540, 0544); `--heavy` runs it on your omp `slow` role, from its plan through its execution (0590) |
| `jocasta thread list [<project>] \| status <thread> \| send <thread> "<text>" \| stop <thread>` | lists threads, shows one, sends it a message or stops its run; `send` and `stop` ask your Allow first (0544) |
| `jocasta thread finish <thread> merge\|pr` | merges the thread or opens its PR, after your Allow (0426, 0544) |
| `jocasta worktree list \| remove <path>` | lists leftover worktrees, or removes one after your Allow (0456, 0544). The `thread` and `worktree` commands exit 1 when refused or failed (Deny included), 2 on bad usage and 3 when Jocasta isn't running |
| `jocasta --version`, `-v` | `jocasta <version> (omp <version>, herdr <version>, bun <version>)` (0052) |
| `jocasta --help`, `-h` | the usage |

## Config

- omp's own config stays in `~/.omp/agent` (0387). `config/omp-config.yml` is the reference for your `/settings` pass (0420, 0451, 0593): besides the earlier settings it sets `disabledProviders: [claude-plugins]`, `modelRoles.web: web/duckduckgo`, `find.enabled: "off"`, `startup.setupWizard: false`, `startup.changelogMode: hidden`, `github.enabled: true`, `collab.autoStart: "off"`, `secrets.enabled: true`, compaction at 300k tokens with a handoff first (0448, 0592), a warning once a session has spent $50 (`session.warnCost: 50`), folders where every action asks you first (`tools.restrictedFolders`: `~/.ssh`, `~/.aws`, `~/.azure`, `~/.docker`, and on Windows setup adds `%SystemRoot%` and `%ProgramFiles%`; 0594), `providers.openai-codex.daybreak: false` and `spotlight.experiment.enabled: false`.
- Outside `config.yml`, written by `jocasta setup` as a diff after your `y` (0451, 0479, 0556, 0594): `~/.omp/agent/keybindings.yml` gets `app.live.toggle: []` (Ctrl+L no longer starts omp's live voice) and `app.stt.toggle: f9` (the key Right Ctrl sends), `~/.omp/agent/lsp.json` gets `"idleTimeoutMs": 300000` (your other keys kept in both, and your own value for Jocasta's key kept and shown); `~/.omp/agent/WATCHDOG.md` (the advisor's brief) comes from `config/WATCHDOG.md` when missing, and one you changed is shown and kept, never overwritten; `~/.omp/agent/secrets.yml` comes from `config/secrets.yml` when missing, an empty list for your own strings to hide, and once you change it setup never shows or overwrites it. `jocasta uninstall` takes out only the keys setup added, and `WATCHDOG.md` and `secrets.yml` only while each is still Jocasta's text. Your own strings never go in a repo.
- Each project and thread pane also gets `config/guard.yml` (approvals: `bash.gitGuard`, 204 deny rules, then 55 prompt rules; 0450, 0476, 0485, 0580, 0584) and the project's overlay `<data>/projects/<pid>/omp.yml` (its own memory file, compaction, `AGENTS.local.md`, the lessons skill, and the privacy settings `dev.autoqa: false`, `disabledProviders: [claude-plugins]`, `modelRoles.web: web/duckduckgo` whatever your global config says; 0447–0449, 0459, 0478; the reviewer runs on your `slow` role, 0488). The boss's `config/boss.yml` sets the same privacy settings and omp's dictation (`stt.enabled`, `stt.submitTrigger: release`, `modelRoles.dictation: local/parakeet-tdt-0.6b-v3`, no fallback; 0556). Every pane's overlay hosts its session in Jocasta's collab relay on `ws://127.0.0.1:47499` and keeps `/share` on that port, and a pane starts only if its omp resolves the relay to loopback (0536, 0561). What the guard doesn't stop is in its own section below.
- Private project notes go in `AGENTS.local.md` at the project root, gitignored; every pane reads it (0449).
- Jocasta's files are in `~/.jocasta/agent/` (`JOCASTA_CODING_AGENT_DIR` overrides it): `jocasta.yaml` (settings), `projects.yaml` (watched roots), `herdr.toml` and `herdr-plugin/` (written by setup), `setup/` (setup's markers) and `compose.yaml` (written by Jocasta for the containers).
- `jocasta.yaml` keys: `service.port` (default 47400), `service.quietHours` (before 0584 they were `frontier.port` and `frontier.quietHours`: still read, and `service` wins where both are set), `dashboard.weather` (a place or `latitude, longitude`, shown in `today`'s header), `pollers.github.everyMinutes`, `pollers.azureDevOps.orgs|everyMinutes`, `pollers.calendar.everyMinutes`, `pollers.news.everyMinutes` and `pollers.news.feeds` (brutalist.report lists; by default every Hacker News and LWN link and the newest 10 gaming; 0442), `graph.main` and `graph.small` (an omp `provider/id` for the graph's model calls; omp's roles by default). `frontier.autostart` and `frontier.dashboard.display` are gone; a file that still sets them is told so (0557).
- A problem in `jocasta.yaml` is said once at start, as a notification and in the log, with file and line, and turns off only its own feature: `settings: jocasta.yaml:12: unknown key gihub in pollers; ignored`; a bad `pollers.github.everyMinutes` turns off that poller alone ("; the GitHub poller is off"), a bad `graph.main` Graphiti's model calls ("; Graphiti's model calls are off"); a file that isn't YAML turns off all its settings (`jocasta.yaml:<line>: <reason>; its settings are off`; 0584). A broken `projects.yaml` stops the command with `jocasta: <file>:<line>: <reason>` (exit 1; 0582).
- mise (0473, 0476, 0477): with mise installed, each project can have a Jocasta-kept config `~/.config/mise/config.jocasta-<project id>.override.toml`, never committed; every pane of that project gets `MISE_ENV=jocasta-<project id>`. When a project has none, the boss offers a setup thread that writes it with `test` and `verify` tasks. A repo's own `mise.toml` is used only when trusted under mise's paranoid mode; the only config Jocasta trusts is a thread worktree's unchanged copy of one you trusted in the main checkout, before the thread's pane starts (0596).
- Data: `%LOCALAPPDATA%\jocasta\data` (`JOCASTA_DATA_DIR` overrides it); Jocasta's log is `<data>\logs\jocasta-YYYY-MM-DD.log` (7 days kept; 0431). The service's lock is `<data>\service.lock` (0565), stored check results are in `<data>\checks\` (0542, 0546) and the code index keeps its data in `<data>\code-index\`, which codebase-memory-mcp refuses outside your user folder on Windows, so the data folder must be under it for the index to work (0562).

## What the guard doesn't stop

The guard is a list of command shapes that omp refuses or asks about before running (0450, 0485). It stops the usual ways of losing work or a login: force pushes, deleted remote and force-deleted local branches and refs, `reset --hard`, `clean`, discarding checkouts, dropped stashes (asked first), repo and release deletion through `gh`, recursive deletes, piping downloads into a shell, mise's trust and global changes, git aliases, and reading or printing tokens and credential files (`gh auth token`, `gh auth status -t`, `az account get-access-token`, omp's logins as `omp auth export` and its `agent.db`, gh's `hosts.yml`, SSH keys, Jocasta's `jocasta-service.json` and its old name `frontier.json`). It also catches `git.exe` and a full path to git, and PowerShell's `rm -Recurse` and `ri` forms.

Record 0580 widened it. Denied: combined force flags (`push -fu`, `-uf`, `-fv`, `-vf`, `-fq`, `-qf`), pushes to `:main` or `:master`, the git rules with the program written `GIT` or `Git` (Windows runs both as git), `restore --worktree`, `-W` and `--source`, `update-ref` on `refs/heads/`, `git credential fill` and `git credential-manager get`, `docker inspect` on Jocasta's containers and `docker exec` into them or FalkorDB, omp's own critical shapes (`<(curl`, `<(wget`, `eval $(curl`, `eval $(wget`, piping into `zsh` or `fish`, `sudo rm`), `gh repo edit --visibility`, and omp's logins as `omp auth export` and its `agent.db` (with its `-wal` and `-shm` files), where omp keeps them now instead of `auth.json`. Asked about (and refused in subagents): `gh pr merge`, `gh release create`, and `npm`, `pnpm`, `bun` and `cargo` `publish`. The alias rules match `git config alias.…` and `--config-env`, so fewer harmless `git -c` commands are caught.

A thread's `done_when` commands are run by Jocasta, outside omp, so each is checked against the same rules before the thread starts, at every door (0580). A denied one refuses the start ("thread_start: done_when `<cmd>` is denied by Jocasta's guard (`<rule>`); no thread was started."). One the guard asks about makes the boss's start wait for your Allow, whose box lists each command ("It runs when the work is reviewed:  `bun test auth`"); `jocasta thread new` asks nothing, so it refuses such a command: "jocasta thread new: done_when `<cmd>` needs your Allow under Jocasta's guard, and jocasta thread new asks nothing; have the boss start it. No thread was started."

It is pattern matching, not a boundary. Every pane runs omp as you, with your logins, under omp's `yolo` mode. So the guard can't stop:

- a script the agent writes first and then runs, or an interpreter one-liner beyond the shapes listed in `config/guard.yml` (`python -c`, `node -e`, `bun -e` deletes are asked about; anything else runs);
- shell commands run through omp's `eval` tool (Python or JavaScript), which skip `bash.patterns` entirely;
- a dangerous command hidden in a way the shapes don't match (an unusual flag order, a variable holding the path, a command read from a file);
- anything that reaches your logins or other projects through a tool rather than a command.

It guards the agent's commands only: a command you run yourself in a pane with omp's `!` runs as typed, without the guard (seen in the campaign's re-test, 0481).

A thread that reads a prompt-injected README, test output or web page can therefore still do what you can do. Until thread panes run under an OS boundary (a restricted token, a separate user or a container, without your logins and with limited network; after v1, 0485), treat the guard as a seatbelt and plan approval as the real gate: read the plan before approving it, and keep client repos' own hooks and `.env` files in mind.

A thread can also drive herdr through its socket as you can: start the plugin's actions and press keys in an open popup (0552, 0568). It can open a question but not answer Allow, Merge, PR or Stop everything, which take letters only the popup shows; herdr's `pane read`, `agent read` and snapshot don't show a popup's screen (0558). The letters also come from the service's local API, to a program that holds the service's token (in a file in the data folder) and the question's one-time code (in the popup's environment). Each question has one reader (0578): the popup reads it first and gets a secret its answer must carry; any later read ends the question as its fallback (Deny, or Cancel) and shows "Jocasta — Another program read your question "…"; it counted as Deny.", and the boss is told. So a program that takes the code from the running popup gets nothing, and you see that it tried. It doesn't stop a program running as you that gets the token and the code and reads the question before the popup does, in the moment between the popup's start and its first read: that program can answer, and the popup finds the question gone. Nor does it stop a program running as you from doing by other means what the choice would have done.

The canvas page allows eval (`'unsafe-eval'`) so whiteboard's software maps can route their lines with libavoid (0493); inline scripts and origins other than the service and whiteboard's server on loopback stay blocked. Content that reaches the canvas (a diagram or review an agent draws) is therefore easier to turn into running code than it would be without eval; rebuilding libavoid without eval is open for after v1.

## `/share`

`/share` is omp's. In Jocasta's panes and the boss, `share.serverUrl` points at the local relay's port, which serves nothing but the relay's rooms, so a plain `/share` there never leaves the machine (0561). In an omp outside Jocasta it uploads the session as an encrypted blob to `my.omp.sh/s` by default; with `--gist` it makes a GitHub gist. Then the whole conversation leaves the machine, client code included (0047). Think before you run it.

## Linux and macOS

Builds exist for linux-x64 and darwin-arm64 (`scripts/install.sh`, installing to `~/.local/share/jocasta/`), with the same service and herdr plugin. `install.sh` refuses a folder that isn't Jocasta's as `install.ps1` does, unpacks beside the install (`<folder>.new-<stamp>`) and swaps it in, so no stale file stays (0579); `jocasta uninstall` removes what it put there. Right Ctrl dictation, the terminal reopened for the close question (it waits for the next `jocasta` instead), the calendar's Credential Manager entry and uninstall's check for omp sessions running from the install are Windows-only. The memory graph needs Docker Desktop there (see Prerequisites). CI runs the test suite on Ubuntu and macOS as on Windows (0572); the builds themselves haven't been run there (0481).

## Where things are

- `docs/plan.md`: the final plan. The vision in its section 1 is the source of truth (0019).
- `docs/decisions/`: the decision log and feature register.
- `AGENTS.md`: the rules every agent follows here.

## Development

Requires Bun 1.4.3 (0036, 0042, 0435). With [mise](https://mise.jdx.dev) the repo's `mise.toml` pins Bun 1.4.3 and Node 24.21.0 (`mise.lock` holds their URLs and checksums; 0476). Under Jocasta's mise file (paranoid trust) it needs `mise trust mise.toml` once in the checkout, and again after each change to it; Jocasta's threads trust their unchanged copy themselves (0596):

```powershell
mise install --locked
mise run check                       # typecheck, lint and the tests
mise run build                       # bun run build
mise run evals                       # bun run evals
mise run smoke                       # the herdr and Graphiti smokes on this machine
```

Or with Bun directly:

```powershell
bun install
bun run typecheck                    # tsc --noEmit (0037)
bun run lint                         # biome check
bun run format                       # biome check --write
bun run test                         # bun test --parallel --timeout 30000; the canvas page's tests need Microsoft Edge and skip without it
bun src/entry.ts                     # a dev run of the CLI
bun run build --target windows-x64   # also linux-x64, darwin-arm64 (0043)
```

Opt-in smokes: `JOCASTA_HERDR_SMOKE=1` (a real herdr), `JOCASTA_GRAPHITI_SMOKE=1` (Docker), `JOCASTA_OMP_GATEWAY_SMOKE=1` (a model call through your omp's `auth-gateway stdio`, 0463). Live checks set `JOCASTA_HERDR_SESSION` to a throwaway session name so they never touch session `jocasta` (0441). Evals run omp on real cases: `JOCASTA_EVALS_REAL=1 bun run evals` (0375).

`bun run build` writes `dist/<target>/` (`jocasta.exe`, `config/`, `node/`, `whiteboard/`, `whiteboard-canvas/`, `editor.worker.js`, `hunk/`) plus `dist/jocasta-<target>.zip|.tar.gz` and its `.sha256` (0044); for a target other than this machine's, `node/`, `whiteboard/` and `hunk/` are left out. whiteboard is built from the submodule `forks/whiteboard` at v0.2.4 (`git submodule update --init`; 0417, 0564). The `today` pane and the `ask` popup are compiled into `jocasta.exe` with OpenTUI. Pushing a `v*` tag publishes a GitHub release (0051).

## Credits

- [herdr](https://github.com/herdrdev/herdr) (Apache-2.0): the work surface; its Tokyo Night is Jocasta's palette.
- [oh-my-pi](https://github.com/can1357/oh-my-pi) (MIT): the agent in every pane, and dictation with its local Parakeet TDT 0.6B v3 model (0556).
- [OpenTUI](https://github.com/anomalyco/opentui) (`@opentui/core`, MIT): the `today` pane and the `ask` popup (0551).
- [whiteboard](https://github.com/devdotfast/whiteboard) (MIT): the canvas and its server, shipped with its LICENSE (0393).
- [hunk](https://github.com/modem-dev/hunk) (MIT): the review pane (0394).
- [Monaco Editor](https://github.com/microsoft/monaco-editor) (MIT): the canvas's code peeks and diffs (0411).
- [codebase-memory-mcp](https://github.com/DeusData/codebase-memory-mcp) (MIT): the code index, installed through mise (0542, 0562).
- [Graphiti](https://github.com/getzep/graphiti) (Apache-2.0) with [FalkorDB](https://github.com/FalkorDB/FalkorDB), in containers (0334).
- [Node.js](https://nodejs.org) (MIT), shipped with its LICENSE to run whiteboard's server.
- [Ponytail](https://github.com/DietrichGebert/ponytail) (MIT), in `config/ponytail.md` (0421).
- [Mona Sans](https://github.com/github/mona-sans) by GitHub and [Geist Mono](https://github.com/vercel/geist-font) by Vercel, both under the SIL Open Font License 1.1; the font files and their licences ship in `src/frontier/canvas/fonts/` (0369).
- News headlines from [brutalist.report](https://brutalist.report) (0429); weather from [Open-Meteo](https://open-meteo.com) (CC BY 4.0).
