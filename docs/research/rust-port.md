# The Rust port: facts for its decisions (2026-11-15)

Record 0625 rewrites everything Jocasta owns in Rust before the benchmarks, and puts the port's own decisions first. This file holds the facts those questions used, verified on 2026-11-15. The decisions are 0627–0639: toolchain, order and cutover, layout, tests, the crates, and what of `bench/` and `evals/` is ported. The raw reports are outside the repo, in `D:/jocasta-spikes/rust/` (`inventory.md`, `crates.md`); every crate figure there carries its crates.io, docs.rs or RustSec URL. Crate counts are transitive crates on Windows from `cargo tree`, measured in a throwaway project with its own Cargo home.

## What is ported

- **Size of the code.**
  - `src/`, without the canvas: 27,976 lines of TypeScript.
  - The canvas host page: 1,996 lines. It stays TypeScript because it runs in his browser.
  - `config/pane-check-extension.ts` stays TypeScript because omp loads it (0617).
- **Tests.** 103 test files. 99 of them import `src/` internals, so they don't carry over as they are. The black-box ones drive the `jocasta` process or its HTTP surface: `test/entry.test.ts`, `test/frontier/server/assets.test.ts`, the canvas test, the evals' `fetch` test, and the bench script tests.
- **Fakes** (`fake-herdr`, `fake-mise`, `fake-hunk`, `fake-gh`, `fake-omp`, `fake-hosts` and the evals' CLI fakes) are separate programs or sockets the product talks to. A Rust binary can be tested against them as they are.
- **What `bench/`, `evals/` and `scripts/` import from `src/`:**
  - the herdr client;
  - `proctree`, `service-file` and `launch`;
  - `checks`, `sessions` and `ledger`;
  - `fsx`, `process`, `guard`, `jsonl` and `pathkey`;
  - the layer module A6 loads (`src/frontier/layer/bench.ts`).

  H1's oracle loads omp's own TypeScript (`bench/h1-guard/omp-verdicts.ts`).
- **Runtime facilities the port has to replace.**
  - Bun: `Bun.serve` with websockets, `Bun.spawn`, `bun:sqlite` (FTS5), `Bun.YAML`, `bun:ffi` (Credential Manager, the process walk, the Right Ctrl keyboard hook), `fs.watch` and `Bun.stringWidth`.
  - npm: `@opentui/core`, `node-ical`, `typebox`.
- **Surfaces the port must keep** (plan appendix I):
  - 15 HTTP routes and the state-feed websocket on 127.0.0.1;
  - the collab relay (websocket rooms on port 47499);
  - the MCP stdio server, today hand-written JSON-RPC in 492 lines;
  - the herdr plugin roles;
  - the CLI words and exit codes;
  - the SQLite catalog with its migrations;
  - YAML.

  Jocasta writes YAML in two places only, both its own generated files (`src/setup/omp-settings.ts:77`, `src/setup/setup.ts:305`). It edits his files line by line, without a YAML library.
- **The hardest pieces:**
  - the Right Ctrl hook, a low-level keyboard hook with its own message pump;
  - process identity: Toolhelp, `GetProcessTimes` and the PEB cwd on Windows, `/proc` on Linux, `ps` on macOS;
  - Credential Manager;
  - the service's start and stop ordering;
  - the herdr client, with its subscriptions, `events_lost` and the retries for dropped requests;
  - SQLite migrations shared between processes;
  - the Windows upgrade in `install.ps1`;
  - the build pipeline (whiteboard at a pinned commit, Node and pnpm);
  - the terminal line model: widths and graphemes.

## Toolchain

- **On the PC:** rustc and cargo 1.93.1 with rustup 1.28.2.
- **Current stable:** Rust 1.100.0 (2026-11-12). `rustup check` offers the update.
- **Package age.** Cargo 1.100.0 adds a minimum package age: `[registry] global-min-publish-age = "7 days"`. A version younger than that is ignored unless it is already in `Cargo.lock`. Cargo 1.93.1 ignores the key.
- **MSRV.** Two candidate crates need a newer rustc than 1.93.1: sqlx 0.9.0 (1.94) and sysinfo 0.40.3 (1.95).
- **herdr** pins Rust 1.96.1 in `rust-toolchain.toml`.

## Crates per need

Transitive crate counts are measured on Windows with `cargo tree`.

| Need | Candidates (latest, age in days, transitive crates) | Notes |
|---|---|---|
| Async runtime | tokio 1.54.1 (24 d; 12 with minimal features); std threads (0) | rmcp, axum, reqwest and tokio-tungstenite are async-only; herdr, zellij and atuin use tokio |
| HTTP and websocket server | axum 0.8.9 (214 d, 48); hyper 1.12.0 (39 d, 12 for `server,http1`); tiny_http 0.12.0 (1,500 d, 5); actix-web (138, four advisories on 2026-11-06) | hyper and axum drop a handler when its client disconnects: the pane check's cancel needs that; tiny_http can't see it |
| HTTP client | ureq 3.5.2 (3 d, 26; blocking); reqwest 0.13.5 (67 d, 85–102) | |
| MCP stdio | hand-written JSON-RPC with serde_json (as today); rmcp 3.10.0 (3 d, 68; about 20 releases since August, four advisories in its HTTP transports) | |
| YAML | saphyr 0.3.4 (4 d, 20; YAML 1.2, loader and emitter); serde_norway 0.9.42 (693 d, 9; serde); yaml-rust2 0.13.0 (65 d, 7); serde_yaml deprecated; serde_yml unsound and unmaintained | none preserves comments except yaml-edit (young, untried) |
| SQLite | rusqlite with `bundled` (SQLite built with FTS5) | sqlx 0.9 needs rustc 1.94 and can't share one SQLite with rusqlite |
| iCalendar | icalendar 0.17.17 with rrule 0.14.0; ical 0.11.0 (parser only) | time zones across DST not tested |
| Terminal UI | ratatui 0.30.2 with crossterm 0.29 (62); crossterm alone (21) | herdr uses ratatui and crossterm; `today` and `ask` draw lines through a 200-line model of their own today |
| Windows APIs | windows-sys 0.61.2; windows 0.62.2 | herdr uses both; windows-sys has the JobObjects module |
| Process groups | process-wrap 10.0.1 (Job Objects, process groups); command-group is superseded | |
| CLI words | Jocasta's own routing (`src/shared/route.ts`); lexopt (1 crate, 37 KiB); clap (21, 574 KiB) | |
| File watching | notify 8.2.2 (7 d, 10); removed by 0642, as no module watches files | |
| JSON | serde_json 1.0.154 | |

## Precedents

- **herdr** is Rust: tokio, ratatui 0.30, crossterm 0.29, interprocess, windows-sys with JobObjects, clap trimmed, toml_edit. It has no HTTP server, YAML, SQLite or MCP crate.
- **zellij and atuin** also use tokio and crossterm 0.29.
- No published project combines a localhost HTTP service with long-held requests, a TUI and an MCP stdio server.

## Measured combinations (2026-11-15)

Each count is the number of transitive crates on Windows (`cargo tree --target x86_64-pc-windows-msvc -e normal`), without the root package. The base is tokio, rmcp (`server`, `transport-io`) and serde_json: 61.

| Set on the base | Crates |
|---|---|
| hyper (`server`, `http1`), hyper-util, tokio-tungstenite, ureq | 106 |
| the same with reqwest (`rustls`, `json`) in place of ureq | 138 |
| axum (`ws`), ureq | 118 |
| axum (`ws`), reqwest | 145 |

The next table adds one crate set at a time to the hyper-and-ureq set (106).

| Added | Crates added |
|---|---|
| crossterm, unicode-width, unicode-segmentation | 15 |
| ratatui | 49 |
| yaml-rust2 0.13 | 5 |
| saphyr 0.3.4 | 11 |
| serde_norway | 3 |
| rusqlite 0.42 `bundled` | 7 |
| icalendar 0.17 `recurrence` | 13 |
| clap, trimmed | 4 |
| lexopt | 1 |
| windows-sys, process-wrap, notify | 18 |

## Differences from the TypeScript build

Every difference the porting agents listed (`rust/differences.md`), by module. **fixed**: Rust now matches the TypeScript, with a test that pins it. **kept**: it stays, for the reason given.

Decided with the ledger:
- `surface::last_logged_error` swallows a read error of a log file that vanished between listing and reading; the TypeScript would throw out of the surface's start over a log race. **kept** (a race that must not end the surface).
- A dropped `hold_question` release leaves the hold open, as a never-called release does in the TypeScript. Same behaviour.
- chrono-tz is only a transitive dependency of icalendar's `recurrence` feature. No new crate.
- A calendar `VTIMEZONE` with a custom TZID falls back to local time. **kept**: feeds use IANA names or the 44 Windows names covered. Risk: a feed that defines its own zone name (a self-hosted calendar with `TZID:Company/HQ`) shows its events at the wrong hour, by the gap between that zone and local time.

### Boss
- `LaunchedPanes` has `&self` methods and `Result` returns: **kept**, no observable output (entries keep the key order, so `panes.json` is byte for byte).
- Optional parameters are `Option`: **kept**, API shape only.
- `Counts` struct for the close question: **kept**, API shape only.
- `allow_question` takes `message` and `detail` apart: **kept**, same texts.
- Popup `answer` and `how` are `async fn`s on a watch channel: **kept**, API shape only.
- `ensure_overlay` failure reaches a waiting caller as `Error::other(message)`: **kept**, same message text.
- `by_creation` is a hand-written numeric-aware compare: **kept**, agrees with `localeCompare` on herdr's `w<number>` ids.

### Checks
- Stand-in checks are shim scripts: **kept**, test-only.
- Exit code on POSIX reads `128 + n` for a signal: **kept, not a difference**: Bun's `proc.exited` resolves to the same (probed under WSL: SIGTERM 143, SIGKILL 137), and the TypeScript stores that as `exit`. Pinned by a unix test.
- Spawn failure text is Rust's io error: **kept**, the log line differs only in the OS's wording.
- A failure in a joined run reaches joiners as `Error::other`: **kept**, same message.
- `Weather.code` of a fractional `weather_code` is truncated: **kept**, WMO codes are integers.
- `Weather.at` parses only `YYYY-MM-DDTHH:MM[:SS[.fff]]`: **kept**, Open-Meteo sends that form; other text gives an empty `at` as `NaN` did.
- Schema checks hand-written over `Value`: **kept**, same shapes.
- `every_minutes` is `f64`: **kept**, the schema allows any number from 1.

### Cli
- `terminalClose` runs inside `open_surface` as a task: **kept**, same close, once.
- `last_logged_error`, `AttachedSurface::leave` and `read_dir` errors are swallowed: **kept** (see above); a pid file name must parse as `u32`: **kept**, `1e3` is not a pid file.
- `Jocasta didn't answer: …` carries reqwest's text: **kept**, the wording of a transport fault.
- `left` in `AnswerReply::Wrong` is kept only as a non-negative integer: **kept**, the service sends one.
- A dropped `hold_question` release leaves the hold open: same behaviour (see above).
- The state feed's binary frames are skipped: **kept**, the service sends text only.
- `thread_cli` logs a transport failure with its `Debug` text: **kept**, a log line's detail.
- `normalize_origin` returns `Result`: **kept**, `new URL` can throw; the scp pattern's `.` excludes line terminators as JavaScript's does.

### Config
- The settings parse cache is keyed by file as well as text: **fixed** (0643). One slot keyed by text alone let two roots with the same `jocasta.yaml` text share one parse and its said-marks, so `a_bad_value_turns_off_only_its_own_feature_naming_the_line` failed now and then in a parallel run.
- YAML and JSON syntax-error wording is the parser's own: **kept**, Bun's text can't be reproduced; the `file:line: ` prefix and the exit code are tested.
- `omp_yaml`: **fixed**. Pinned against 6,421 cases of Bun 1.4.3's output (`src/setup/omp_yaml_corpus.json`). Changed: case-exact reserved words, a number scan learned from Bun (`1e`, `.`, `0-0`, `...` and `---` endings quoted), a leading `<` or BOM, NBSP written `\_`, other Unicode spaces and C1 controls left plain, JavaScript number text (`123456789012345680`, `-0`), and array-index keys first. **kept**: a byte order mark past the first character is quoted here and plain in Bun (our own parser refuses it plain); cosmetic, still round-trips. Every corpus string also reads back as itself through `yaml::parse_value`.
- Settings validation in plain code (no typebox): **kept**, same problem lines and messages (compared on `roots[0].depth must be >= 1` and the like).
- `Settings` holds its lines and problems as fields: **kept**, API shape only.
- Unknown marker kinds: **fixed**. `read_marker` keeps an `undo` entry of an unknown kind as `Undo::Unknown`, and `write_marker` writes it back as it was; uninstall fails on it as the TypeScript does (`no undo kind …`).
- A `port` or `pid` that isn't an in-range integer reads as no service file: **kept**, the service writes integers.
- `service_health` caps the connect at 500 ms: **kept**, Windows takes 2 s to refuse a closed port, which read as busy.
- Defaulted parameters are `Option`: **kept**, API shape only.

### Core
- `DbError` is `user_with_code(msg, 1)`: **kept**, prints and exits as the TypeScript's.
- `self_command` in a source run is the running exe: **kept**, `[bun, src/entry.ts]` has no Rust twin.
- `CATALOG_MIGRATIONS` writes `threadsTable("'home'")` out: **kept**, identical SQL.
- `day_start` of a malformed day uses the destructuring defaults: **kept**, `NaN` can't be an `i64`.
- `day_label` prints `Sept`: **kept**, it is what Node and Bun print.
- `paths` are lexically normalized: **kept**, as Node's `join`.
- `write_atomically` retries Windows access errors: **kept**, Rust maps them to several kinds.
- `web_link` uses the `url` crate: **kept**, no differing case found.

### Herdr
- `HerdrError` is its own type: **kept**, `code` is matchable.
- A failed `prepare` reaches the caller as `HerdrError` code `"error"`: **kept**, same message.
- Socket and pipe names are `String`: **kept**, they are pipe names on Windows.
- `env`, `tokens` and `state_labels` are ordered pair lists: **kept**, keeps the JSON key order sent to herdr.
- The Windows pipe connect retries `ERROR_PIPE_BUSY` every 50 ms up to 30 s: **kept**, as libuv's.
- Connect-error text mimics Node's `Error: connect ENOENT <path>`: **kept**, other errors use Rust's wording.
- A server that can't start gives a `didn't start` error after 10 s: **kept**, the TypeScript threw an unhandled `error` event.
- `pane_processes` reads in a sliding window of 4, in pane order: **kept**, only the interleaving of reads differs.
- `clean_text` works on `Value`: **kept**, the TypeScript is generic.

### Pollers
- Calendar expansion follows node-ical's `expandRecurringEvent`: **kept**, same windows, exceptions and overrides.
- A `RECURRENCE-ID` with no series counts as a meeting each: **kept**, matches the SHR-4 comment; node-ical shows only the first per UID.
- A feed icalendar can't parse: **fixed**. The feed is first read as node-ical reads it (lines unfolded, a line that isn't `name:value` skipped, `TZID` quotes dropped); text that still isn't iCal is an empty list, and a second `DTSTART`, `DTEND` or `DUE` in one component is `calendar: the feed isn't iCal`, where node-ical throws. A feed with `BEGIN:VEVENT` but no `VCALENDAR` is an empty list (node-ical would list it): **kept**, no feed does that.
- Timezones go through chrono-tz and 44 Windows names; an unknown TZID falls back to local time: **kept** (see above; names the risk).
- RDATE is ignored: **kept**, as in node-ical.
- A full-day `UNTIL` is normalised to the day's end: **kept**, as the expansion does.
- Duplicate non-override events with one UID: the first wins, no SEQUENCE merge: **kept**, as in node-ical.
- `run_json` parse errors carry serde_json's text: **kept**, reaches only `PollRound.failures` for Azure.
- GitHub and Azure payloads are parsed into structs: **kept**, a wrong-typed field fails the round where the TypeScript passed it through.
- `cpu_times` and memory per platform: **kept**, `GetSystemTimes`, `/proc/stat`, Mach FFI and `df -Pk` read what `node:os` and libuv do; other unixes can't read memory.
- `\s` in the link patterns lacks U+FEFF: **kept**, negligible.

### Proc
- `ProcessId.start` is `f64`: **kept**, a FILETIME rounds as a JavaScript number does. serde_json `float_roundtrip` is now on in the repo: **fixed**, a FILETIME above 2^53 read back one ULP off, and a live `service.lock` holder looked like another run.
- Snapshots and parent maps are `BTreeMap`: **kept**, walk order as the TypeScript's tests.
- `AbortSignal` is a watch receiver: **kept**, API shape only.
- `run_program` awaits the tree kill on abort: **kept**, the program is gone before the error returns; no output differs.
- `run_program` timeout `signal`: **fixed**. Bun's `signalCode` after the tree kill is `SIGKILL` on POSIX and none on Windows (probed on Windows); Rust now gives the same. `run_program_sync` at its limit gives `SIGTERM` on every platform, as `spawnSync` does (probed). The readers (`mise`, `hunk`, `quota`) test `timedOut` first, so no text changes.
- A program that can't start: **fixed**. The error reads `Executable not found in $PATH: "x"`, `ENOENT: no such file or directory, uv_spawn 'x'` for a path, and `spawnSync x ENOENT`, as Bun and Node say them (the text reaches mise's log lines).
- `run_program_sync` output past `max_bytes` is an `Io` error `spawnSync ENOBUFS`: **kept**, as Node's `maxBuffer`.
- `take_calendar_env` removes the variable in an `unsafe` block: **kept**, edition 2024; called before any thread starts.
- `LineRpc` (`exited()` is `-1` for a signal, `kill()` ends the direct child, `unref()` does nothing, `Value` messages, the exit error rebuilt per call): **kept**, no output differs.
- Jocasta's own programs (`run_program`, `run_program_sync`, the checks) run in a process-wrap Job Object (a process group elsewhere), and their time limit or abort ends the whole job: **changed**, per 0641. A grandchild whose parent already exited now ends too; the TypeScript snapshot walk missed it (0486, 0490), and `spawnSync` ended only the direct child at its limit. The hunk daemon starts in a job or group with no kill on close, so it outlives Jocasta as before; the walk stays for trees Jocasta did not start.
- A zombie on Linux: **fixed**. `pid_alive` read `/proc/<pid>` existing, so a killed child its parent hadn't waited for (a zombie) still counted as running, and nine tests failed on their first Linux run (0643). Bun's event loop reaps at once, so the TypeScript never saw one. `/proc/<pid>/stat`'s state `Z` or `X` now means ended, as `process_start` already took it.

### Routes
- Bun's `Server` becomes `ServerHandle` plus axum's upgrade: **kept**, API shape; no idle timeout in axum.
- `stop(true)` fails in-flight bodies so the connection aborts: **kept**, as Bun's.
- Handler panics answer `500 Internal error`: **kept**, as the `error` hook.
- The lock file's `start` is an integer when whole: **kept**, as JavaScript writes it.
- `take_instance` times waits on a monotonic clock: **kept**, immune to clock changes.

### ServerBase
- A `.cmd` shim's path is quoted when it has spaces, so the stand-in strips `"`: **kept**, test-only.
- `String(error)` prefix in `mise.rs`: **fixed**. The "did not start" result and the "failed" log line carry `Error: ` (`UserError: ` for a refusal), as `String(error)` does.
- Relay bind failure carries the OS's text: **kept**, the wording of a fault.
- Relay `stop()` aborts the listener task, so the port frees a moment later: **kept**, nothing reads it in between.
- `monaco_worker`: **fixed**. It serves the embedded `MONACO_WORKER` (record 0639) with the TypeScript's one `content-type: text/javascript; charset=utf-8` header and no cache header; the install-folder lookup is gone.
- `canvas_asset` returns metadata, and `None` for a directory: **kept**, the TypeScript would build a broken response.
- Whiteboard `stop()` kills the whole process tree, `ready` is a watch channel, failures are `Error::Other`: **kept**, Node can sit behind a shim.
- The `mise.rs` regexes use `[^\n\r\u2028\u2029]` for `.`: **kept**, the same matches.
- The relay overlay test uses `yaml::parse_value` for `Bun.YAML.parse`: **kept**, test-only.

### Sessions
- Unknown times are `Option<i64>` and sort last: **kept**, `null` in JSON as `NaN` became; counts are `u64` and a negative one reads 0.
- A pricing window of 0: **fixed**. `PeakContext` keeps `window: 0` with `percent: null`, and the line names no window.
- `date_parse` takes RFC 3339, a bare date and a local date-time: **kept**, the forms omp writes.
- `toFixed(2)` rounds exact ties half up as JavaScript does: **kept**, not a difference.
- Regex edge cases (`\s` lacks U+FEFF, surrogate entities left as written, `(?m)^` splits on `\n` only): **kept**, negligible.
- `arg_hash` prints non-integer floats with serde_json's format: **kept**, differs only in exponent forms.
- `Date.parse` quirks beyond the three forms: **kept**, same reason as `date_parse`.
- The `DROPPED` pattern's `\1` is a scanner: **kept**, same matches; `lower` is ASCII-lowercased so offsets match.

### SessionsView
- `read` and `warm` swallow an I/O error from a tail: **kept**, a session file busy on Windows must not end the view.
- Times are `i64`, a fractional timestamp is truncated: **kept**, whole milliseconds.
- `children`, `session_file` and `sessions_since` list directories sorted: **kept**, the TypeScript's raw order only decided ties.
- `TESTS:` uses an ASCII `\b` and `(?m)^` after `\n` only: **kept**, as JavaScript's.
- `own_exit`'s `\d*` is `[0-9]*`: **kept**, JavaScript's `\d` is ASCII.
- `SessionTail::mtime_ms` starts as `None`: **kept**, reads as `NaN` did.

### Setup
- No `Bun.TOML.parse`: **kept**, no TOML crate; the manifest and fragment are checked by lines.
- The stand-in herdr check always stays on: **kept**, no test can reach a real herdr.
- Shipped config is read from `assets_dir`, never embedded: **kept**, as `setup.ts` reads it.
- `Bun.YAML.stringify` becomes `omp_yaml`: **fixed** (see Config).
- A non-string compose `name` prints as JavaScript prints a number or boolean, else `jocasta`: **kept**.
- `TOKYO_NIGHT_SCHEME` is built from `frontier::tui::TOKYO_NIGHT`: **kept**, tested against `palette.css`.
- `SetupUndo` is an enum, `jocasta_command` is `Vec<PathBuf>`: **kept**, API shape; `[bun, entry.ts]` has no Rust twin. The enum gains `Undo::Unknown` through the marker fix above.

### Text
- YAML syntax errors use saphyr's text: **kept**, Bun's can't be matched; the line number follows the same rule.
- `parse_yaml_file` takes a validating closure: **kept**, `settings` and `projects_file` build their own problems.
- `report` unknown-field order: **fixed**. `preserve_order` is on, so the first unknown key named is the first in the JSON text (array-index keys first, as a JavaScript object has them), pinned against typebox's own answer.
- The `report` validator is hand-written: **kept**, reproduces typebox's first-error order and messages; lengths count code points.
- Whole-valued floats serialize as `1`: **kept**, as `JSON.stringify`.
- `push_hook_env` returns a `BTreeMap`: **kept**, an environment's order isn't read.
- `PushHookDeps` is a trait: **kept**, API shape.
- YAML aliases read as `null`: **kept**, jocasta's files hold none.

### Tui
- Raw mode, bracketed paste, key polling every 100 ms and end signals through ratatui and crossterm: **kept**, terminal plumbing.
- `ask` keeps OpenTUI's key names, key releases never arrive: **kept**.
- Column widths come from `unicode-width`, graphemes are split by hand: **kept**, agree on CJK and the symbols used; exotic emoji may differ.
- Each frame fills the terminal with `bg0` and clips at the right edge: **kept**; a wide character's second cell resets its background.
- `Math.round` and `toFixed` are re-implemented: **kept**, not a difference.
- An unparseable `since` shows no wait, where the TypeScript showed `NaN min`: **kept**, an improvement on a defect; `resetsAt` as the TypeScript.
- `palette.ts` has no Rust twin: **kept**, `TOKYO_NIGHT` is tested against `palette.css`.
- An empty project list ignores up/down and Enter answers an empty name: **kept**, the TypeScript would produce `NaN`.
- Brief paste collapses whitespace as the TypeScript does: **kept**, not a difference.

### Mains (wave 4)
- Spend state is a `SpendTracker` beside the `SessionIndex`, and new files are read by `spend_settled` off the call: **kept**, same totals and the same one-batch-at-a-time warming as the TypeScript's `WeakMap` and `warming` flag.
- `AskProgram` releases the hold in `cleanup` and on drop: **kept**, every exit path releases once, as `ask/main.ts` does.
- News `headline()` times parse RFC 3339, a local time and a bare date only: **kept**, brutalist.report sends RFC 3339; other forms show no time.
- The health route builds its two responses itself: **kept**, same content types and texts.

### Projects (wave 4)
- `branch_slug`'s NFKD fold is a table generated from Bun 1.4.3: **kept**, matches that Unicode version; a later code point folds to `-`.
- `list_projects` approximates `localeCompare`: **kept**. Risk: names with accents or other scripts may list in a different order than under Bun.
- `cut` and `brief_title` count UTF-16 units but never split a surrogate pair: **kept**, JavaScript would leave a lone surrogate.
- `money` rounds halves away from zero and prints `$0.00` for -0: **kept, not a difference**.
- `Bun.Glob` for `ignore:` is globset with `literal_separator`: **kept**, an invalid pattern errors as Bun's did.
- Directory names sort by code point: **kept**, differs from UTF-16 order only for names mixing astral and high-BMP characters.

### Uninstall (wave 4)
- OS error texts are Rust's (`remove_tree`, `read_dir`, spawn failures): **kept**, the wording of a fault.
- The MCP entry fingerprint hashes serde_json's text: **kept**. It differs from `JSON.stringify` only for whole floats like `1.0`, which no MCP entry setup writes holds.
- `describe_process`'s Windows path helpers are hand-written; a drive-less rooted word takes the install's drive: **kept**, Node would take the working folder's.
- A `processes_under` line whose pid isn't a number is skipped: **kept**, JavaScript kept a `NaN` pid nothing could match.
- `install_contents` sorts by UTF-16 code units: **kept, not a difference**.
- `unsafe_folder` uses `pathkey::resolve`: **kept**, the one resolver.

### Allow (wave 5)
- `AllowDenied` is `AskError::Denied`, which converts to `Error::User` with exit code 2: **kept**, same message and code.
- The question's abort is `process::AbortSignal`, and the dialog task sees it and ends itself: **kept**, as the TypeScript's detached promise.
- The race checks a stop first, then the answer, then the timeout: **kept, not a difference**.
- `add_history` counts UTF-16 units and trims as JavaScript does; query words are a hand-written `[a-z0-9]{3,}` scan: **kept, not a difference**.

### Worklog (wave 5)
- `merge_worklog` matches the closing marker by hand instead of a backreference: **kept**, same matches.
- A failed `GitPlaces` lookup isn't cached for the day: **kept**, the next call retries where the TypeScript kept the rejection.
- `ThreadNumbers::of` reads a stored number only as a non-negative integer: **kept**, the service writes integers.
- `remove_checkout`'s reason for non-git failures is Rust's error text: **kept**, the texts that make a refusal final are unchanged.

### Hunk (wave 5)
- The test-declaration pattern's `\1` is three regexes, one per quote, and `#(?!\[)` is `#(?:[^\[]|$)`: **kept**, same matches.
- `\w`, `\b` and the whitespace set follow JavaScript's: **kept, not a difference**.
- `createdAt` parses RFC 3339, a bare date and a local time only: **kept**, hunk writes RFC 3339; other forms count the note.
- A non-object comment list is `Error::Other`, and a hunk that can't start is the spawn's `Error::Io`: **kept**, the wording of a fault.
- The daemon starts with `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP` on Windows and its own process group on Unix: **kept**, it outlives Jocasta with no window, as Node's `detached` and `windowsHide` do.
- The health check uses reqwest without a proxy and with a 1 s timeout: **kept**.

### Places (wave 6)
- `TaskRecord` keeps keys it doesn't know in a flattened map and writes them back after its own: **kept**, no key is lost; the position of an unknown key moves to the end.
- A record from an older build missing a key loads with that key's default and writes it on the next save (`"closed": false`, `"reviews": []`): **kept**, the TypeScript leaves it absent; every reader treats both alike.
- `spend` is written as `JSON.stringify` writes it, and `task.md`'s `$x.yy` rounds ties away from zero as `toFixed` does: **kept, not a difference**.
- `Places`' lookups return `Result` where the TypeScript throws, and its caches need `&mut self`: **kept**, the same failures.

### Watcher (wave 6)
- The TypeScript polls; so does the port, and notify left the port (0642): **kept, not a difference**.
- `watcher.json` is written by hand in `JSON.stringify`'s key order and number form, with inodes rounded through `f64` as Bun's numbers are: **kept, not a difference**.
- A summary's history lines and its indexed mark are one transaction; `add_history` runs in a savepoint so it nests: **kept, not a difference**.
- An explicit `scan()` waits behind the first scan, which takes the scan lock when the watcher starts, where the TypeScript queues it as a microtask: **kept**, the same order.
- The 8 ms slice yields with tokio's `yield_now` instead of `setImmediate`: **kept**.

### Day (wave 6)
- `Day` holds the catalog as `Arc<Mutex<Connection>>`; `queue(|q| …)` and `inbox(|i| …)` stand for the fields, and queue-touching methods return `Result` where the TypeScript throws: **kept**, API shape only.
- The disk alert rounds its percent with `floor(x*100+0.5)`, as `Math.round` does: **kept, not a difference**.
- A reminder or worklog date with a year under 100 is refused ("not a day on the calendar", "no such day"), where `new Date` reads it as 19xx: **kept**, a typed two-digit year is a mistake, not 1925.
- On stop, the HTTP pollers' requests end by dropping their futures, and `gh` and `az` get the day's abort signal: **kept**, nothing reports after the day stops, as with the TypeScript's `AbortSignal`.
- The first poll, quota and leftovers reads run on a zero-delay task after `start()`, not inside it: **kept**; the first machine sample still comes 2 s after start.

### Mcp (wave 6)
- `jocasta mcp` runs on rmcp's `ServerHandler` and `serve` (0632), with the tools built by hand as `Tool` values: the `initialize` and `tools/list` results match the TypeScript build's byte for byte in all three modes (a captured fixture, key order included). **Kept, not a difference.**
- rmcp owns the lifecycle, so these replies change, none of them for a request omp sends: **kept**, the record's choice:
  - Before the handshake, `tools/list` and `tools/call` get `-32602`; rmcp serves nothing until `notifications/initialized` arrives, and `initialize` requires `capabilities` and `clientInfo`.
  - An unparsable line gets no reply, where the TypeScript replied `-32700`. A well-formed non-request gets `-32600` "Invalid request" with no `id`, and an error that answers no request has no `id` key instead of `"id": null`.
  - An unknown method's message is the bare method name. `tools/call` with non-object `arguments` or no `name` gets `-32601` `tools/call`, where the TypeScript said `<tool>: arguments must be an object`.
  - An unknown protocol version gets `2025-11-25`; the TypeScript's object lookup echoed `"toString"` and `"__proto__"`.
- `prompts/list`, `resources/list`, `resources/templates/list` and `completion/complete` answer `-32601` as the TypeScript does, overriding rmcp's empty defaults: **fixed**.
- Each request runs in its own task, so a cheap reply can overtake a slow service call: **kept**, clients match replies by id.
- Calls still running when stdin closes get at most 5 s, then their replies are dropped: **kept**, omp keeps stdin open while a call waits for Allow.
- A failed service call says reqwest's error text, not Bun's; a refused connection still says the service isn't running: **kept**, the wording of a fault.
- JSON nested deeper than 128 levels is a parse error: **kept**.

### Ctx (wave 7: thread, production, events, the context)
- `ThreadRegistry`'s `values()` and `paneIds()` return copies, not live iterators: **kept**, no caller iterates while changing it.
- `Thread`'s optional flags (`merge_after_review`, `catch_up`, `report_asked`, `ThreadPlan.reopened`) are `bool`, false for undefined; `spend_said` stays `Option<bool>`: **kept, not a difference**.
- `Events::thread` warns and returns when `day.hold` fails, where the TypeScript rejects the call: **kept**, the event is lost either way and the warning says why.
- `Events::clean` round-trips a message through JSON; one that fails to clean is dropped with a warning: **kept**, never sent with escapes in it.
- `Places`' hooks run with its state locked and must not call back into it; `sync` runs the catalog sync on a blocking thread with the catalog locked, so other catalog users wait during it: **kept**, the TypeScript's sync ran on its one thread with the same effect.
- `ctx.sessions` is a tokio `Mutex`; a synchronous `DayHooks` reader (`sessions_by_project`) uses `try_lock`, and while `warm` or `prepare` runs it sees nothing new: **kept**, the next tick reads it; the TypeScript reader saw a half-read index there.

### Finish (wave 8)
- The gh fake is a generated `.cmd` (Windows) or `sh` script rewritten from a state, not the test executable run again: **kept**, libtest prints `running 1 test` to stdout, which would corrupt `gh`'s parsed output.
- The 0486 holder process is the test executable run again (`holder_main`): **kept, not a difference**, as the contract says.
- `remove_thread` returns `Result`, because `day.landed`, `thread_unblocked`, `reviewed.mark` and `numbers.forget` are fallible in Rust: **kept**, the TypeScript let those throw too.
- The TypeScript's `FINISH` mode table is a private `Mode` enum: **kept**, API shape only.

### Lifecycle (wave 8)
- The lifecycle tests' world gives every pane a loopback relay (`relay_url_of`), as the TypeScript's `relayTestOptions`: **fixed** (0643). A relaunch ran the real `omp` to read its relay, which only a machine with omp installed has; 14 tests failed without it on Linux.
- The TypeScript's per-pane promise chain is a per-pane queue drained by one task on the lifecycle's own `Calls`, so `settled()` waits for it: **kept**, the same order per pane.
- `handed_off` is `async`, because the session index sits behind a tokio lock; the hold closure for a plan line uses `try_lock` and counts a busy index as "still waiting": **kept**, the line is held a little longer, never lost.
- Hold closures capture a `Link` clone, not the context: **kept**, a queued line doesn't keep the context alive.
- The picker regex uses `(?-u:\b)` so `\b` is ASCII, as in JavaScript: **kept, not a difference**.
- Tests that build a git repo and take the environment lock serialise, so this module's tests add about 50 s to the suite: **kept**.

### Review (wave 8)
- `review_flow_tests`' demo repo sets `user.name` and `user.email` itself: **fixed** (0643). The squash commit the merge makes took the machine's identity, which WSL has none of.
- The `AbortSignal` type is as in the earlier waves: **kept**.
- Methods that return `Result` where the TypeScript could throw (git failures, `reviewed.mark` write errors) propagate the error: **kept**, the same failures.
- The `Promise` chain for pane checks is a spawned task shared as a `futures` `Shared` future, so it keeps running when nobody awaits it, and `stop` waits for it through the part's `Calls`: **kept**, as the promise did.
- No test starts the real daemon; a loopback `/health` answer stands in for it: **kept**.
- The failing-`herdr.prompt` path of `handoff` and the "its review couldn't start" path are untested, because `FakeHerdr` can't make those calls fail: **kept**.

### Status (wave 8)
- `busy_threads`, `hud` and `awaited_panes` return `Result`, because `Day::today()` does; `report()` also returns `Result`, so a failed `today()` propagates instead of being swallowed: **kept**, the TypeScript awaits it too, and its callers (`refresh`, the review's end, a thread's state change) now use `?`.
- The synchronous readers (`day_thread`, `hud`, `write_task`, `ledger_of`) use `try_lock` on `ctx.sessions`; a busy index gives "nothing new yet" (`hud()` the figure last counted, `ledger_of` `None`): **kept**, the next tick reads it.
- Per-thread caches key on the pane id, not the thread object, so a thread replaced in the same pane keeps its entry until its session files change; `forget` clears it: **kept**.
- `capped()` counts UTF-16 units like JS `length` but cuts at a character boundary instead of splitting a surrogate pair: **kept**, a split pair isn't text.
- The weekly review runs through `weekly.review()`, which returns `Result<Option<String>>`; concurrent callers share one future and a failure reaches all of them as `Error::other(message)`: **kept**.
- Tests built through `fake_omp` read the real argv; the shim-based fakes see only the space-joined `FAKE_ARGS`: **kept**.
- The boss notice is said once by reading `PaneLauncher::boss_notice()` and clearing it with `set_boss_notice(None)`, as `status.ts` reads and clears the field: **fixed** (the part had guessed a `take_boss_notice`).

### Procs (wave 8)
- `omp_processes`' pause test kept its temp folder alive (`_folder` named in the destructuring): **fixed** (0643). `..` dropped it at the end of the `let`, and Linux deleted the host registry under the test (Windows kept it, the sleepers' cwd held it).
- A pane's env is the ordered herdr `Env` (a `Vec` of pairs), built in the TypeScript's order: hunk variables, then the hook's count and key/value pairs; mise's env arrives as a `HashMap`, so its keys are added in name order: **kept**, the order of mise's own keys isn't observable.
- `omp`, `pane_omp`, `pane_env` and the boss key are built on first use from the context's options, because the context doesn't exist when the part is made: **kept**.
- `OmpTracker::end`'s second pass runs on a plain `tokio::spawn`, not `ctx.calls.spawn`: **kept**, the TypeScript's unawaited `Bun.sleep` chain was never waited for by `stop`; stop everything ends what it still holds through `end_leftovers`.
- `OmpTracker::poll` starts its read on a task at once and concurrent callers join it: **kept**, as the TypeScript promise did.
- `ensure_boss` builds a new `BossPane` as the TypeScript does, carrying over `state`, `event_at` and `prompted_at` for the same pane: **kept, not a difference**.
- Several tests build more than one `test_context`; the first must be dropped before the second or the env lock deadlocks: **kept**.
- `ServiceOptions::hunk` is `Option<Option<Vec<String>>>`, as `mise` is: `Some(None)` says no hunk is installed. **fixed** (a source run always finds hunk in `node_modules`, so a test of the "hunk isn't installed" refusals (0454, the review key) had no way to arrange it once `PaneLauncher::hunk` was real).

### Gates (wave 8)
- `verdict` returns `Result<PushAnswer>`; only `Error::User` becomes a refusal and other errors propagate, as the TypeScript's rethrow does: **kept, not a difference**.
- The per-repo "asking" map is a `watch` channel per open question; a repo's turn is claimed in one locked step and its entry leaves the map before waiters wake: **kept**, the same turns as the promise chain.
- `thread.state` is read after `awaiting_him` resolves in Review, as the TypeScript does: **kept, not a difference**.
- Two demo repos with the same root commit count as one project (0103); each test repo now gets its own text and message: **fixed** (a flaky test).
- The service now has the real review, so the tests written against the stub changed to the TypeScript's behaviour, each **fixed** to match: a task end opens the review step (phase `review`, the review turn typed) where the lifecycle tests saw it skipped; a passing merge review's outcome is a second boss line (`Finisher::after_review`); the review key's and Finish key's answers count real commits.

### ThreadStart (wave 9)
- `ensureWorkspace[1]` in the TypeScript is `[0]` in the part tests, because there is no boss workspace without the service: **kept**.
- The CO-5 assertion splits the reply on `". "`, not `","`, since the part test brings omp up at once and no half-done `, but …` suffix appears: **kept**.
- If removing the failed start's task folder itself fails, a warning is logged and the original error kept; the TypeScript's `await` threw the removal error instead: **kept**, the start's own failure is the one worth reporting.
- One project's starts run one after another through a per-project `tokio::sync::Mutex`, dropped once no start holds it, where the TypeScript used a promise chain: **kept**, the same order.
- `start_with(project, title, brief, StartOptions { door, done_when, heavy })` is the final shape; `tools.rs` was written against a placeholder `StartRequest` and now calls it: **fixed** (the merge).
- `pane_project` and `note_focus` return `Result`, so `tools.rs` propagates the first with `?` and `refresh.rs` hands the second straight to its step: **fixed** (the merge).

### Refresh (wave 9)
- Single flight is a `Shared` future over two spawned tokio tasks: **kept**, as the promise was shared.
- A step failure goes to `ctx.warn` as `refresh, <name>: <error text>`, once per reason: **kept**.
- `c.finishing.has(ws)` is `finishing(ws).is_some()`, and the cleanup of `"removed"` is `== Finishing::Removed`: **kept**, API shape only.
- `discover` reads `ctx.rediscover` itself instead of a `rediscover()` hook calling in: **kept**.

### Tools (wave 9)
- `big_picture_review` spawns its review with `tokio::spawn`, not `ctx.calls`, as the TypeScript's `void` did, so `stop` doesn't wait up to 30 minutes for it; `worktree_remove`'s leftovers refresh goes through `ctx.calls.spawn`: **kept**.
- `remind` prints the date from `day_label` minus its weekday, so September reads `Sept`, matching the `en-GB` rule in `shared/day.rs`: **kept**.
- The boss-key comparison is a hand-written constant-time compare, since `timingSafeEqual` has no listed crate: **kept**.
- `String(id)` in `inbox_done`'s refusal is hand-written for JSON values; a float with no fraction prints as an integer, as in JavaScript: **kept**.
- The test context's day hook lists no projects, so the `today` tool's "Next" row is empty there; the note test checks the project's inbox instead: **kept**.
- `start_tool` calls `start_with` with `StartOptions`, not the placeholder `StartRequest`: **fixed** (the merge).

### Test world (wave 9)
- `test_context` gives every pane a loopback relay (`ws://127.0.0.1:47499`) before the caller's `options` closure, as the TypeScript's `relayTestOptions`; a test may still override it: **fixed** (0643). Any part test that relaunched or started a thread ran the real `omp` on a machine that has it; the Lifecycle tests' own copy is gone.

### Wave 10 (service, stop sequence, routes, plugin)
- `stop` does not close the catalog: the connection is a shared `Arc<Mutex<Connection>>` that closes when its last holder drops; `Day::stop` is still awaited first: **kept**.
- `cli_call` is the TypeScript's `cliCall`; `notify`'s `show` is `Option<bool>` (`None` shows, as `show !== false`); `JocastaService::start` gives every caller the first outcome as `Error::other(message)`, so a user-error class doesn't carry through: **kept**.
- `StopParts`: `service` is an `Arc<dyn StopService>` so tests stand in a fake; `relay` and `server` are `StopRun` closures that `server.rs` supplies; test-only `ticks`/`timer_count` exist in `service.rs`: **kept**.
- Key refusals while closing: the TypeScript's `Keys` wraps `stop`, `review`, `finish` (after it found its thread) and `start` in the service's `track`, so those four refuse with "Jocasta is closing; nothing was changed."; `finishable` and `newThreadForm` read only and don't. `Keys::track` does it now and `JocastaService::key_stop`/`key_review` no longer refuse on their own: **fixed** (the merge; the wave's report had only `key_stop` and `key_review` refusing, in the service).
- `feed_route` takes a `FeedSocket` (axum's upgrade needs the socket handler; Bun installed it on `Bun.serve`); `FeedService::snapshot` returns `Result`, and an `Err` (the TypeScript's throw) is logged and ends that client's socket: **kept** / **fixed** (the merge).
- A client message that isn't a known `ClientMessage` is ignored (SEC-NET-5); the TypeScript would pass `null` through and throw. The feed breaks on `server.closed()` after sending what was published, so `bye` goes out: **kept**.
- A `null` door body answers 500, as the property read threw; other non-object JSON reads as an empty object: **kept**.
- `request.signal` is an `AbortSignal` that fires when the handler's future is dropped on disconnect; the pane-check door's abort is tested over a socket: **kept**.
- `whiteboard_connection`'s error text is Rust's `Display`; for a canvas manifest read error it differs from Node's: **kept**.
- `close_flow` runs in a spawned task shared by concurrent callers, as the promise ran on: **kept**. `CloseService::busy_threads` is infallible: a failed count is logged and counts nothing where the TypeScript's throw would reject the close: **kept**.
- `PopupHolder::hold(code)` takes no abort signal (Plugin's shape won): axum has none per request, and dropping the returned body is the popup closing, which ends the question as `closed`. Routes' signal parameter and its `Disconnect` guard are gone from the popup routes (the doors still use `Disconnect`): **fixed** (the merge).
- One `Ask` type (`popup_routes::Ask`, `Fn(&PopupView, Option<AbortSignal>, bool) -> Popup`, the bool is `fromKey`) serves the close, the asker and `ActionDeps.ask`, which calls it with `from_key` true; Plugin's `AskFn` is gone. One `CloseOutcome` (`close.rs`, TypeScript `"stopped" | "kept"`) replaces Plugin's `Quit`: **fixed** (the merge).
- `ActionDeps` is built once by `action_route` (its `asking` map outlives each key); `ActionService: KeyService + Notifier` is implemented for `JocastaService`, as are `DoorService`, `FeedService`, `CloseService` and `StopService`; `DoorService` wraps the infallible service calls in `Ok`: **fixed** (the merge).
- Notification body: cut to 239 UTF-16 units plus `…`, never inside a surrogate pair, where `slice` could leave half a pair: **kept**. `run_action` has no `default` case (the enum has no other value) and returns `Result`, refusals as `Error::User`: **kept**.
- `PluginPanes::heal` takes `&self`; each shown question of `AskPopups` is presented by a spawned task, so `show` and `front` need a running runtime: **kept**.
- Wave 11 (`server.rs`) fills: the `Heal` closure over `PluginPanes::heal`; `Ask` from `asker(popups, show)` with `show` over `AskPopups::show`; `PopupParts`/`ActionParts` with `AskPopups` as `PopupHolder`; `ActionParts.quit` and `DoorParts.client_closed` from the close flow; `StopParts.relay` and `.server`; the `shutdown.test.ts`, `service-*.test.ts` and `server.test.ts` cases that need `startTestServer`.

### Wave 11 (server, run, entry, install)
- `start_server` starts relay, herdr, whiteboard, service, stopper, close, routes, listen, then writes the service file; `service.start()` runs spawned and `ready()` gives every caller its first outcome. A failed start undoes the steps in reverse, with no catalog step (it closes on drop): **kept**.
- `ServerOptions` is a flat struct of `Option`s; `whiteboard` is `Option<Option<WhiteboardOptions>>` (`None` the default whiteboard, `Some(None)` off). `shutdown(ask: bool)` and `client_closed()` take the place of `shutdown({ask})` and `clientClosed()`: **kept**.
- `run_service() -> i32`: a `take_instance` error is logged and exits 1, where the TypeScript's promise rejected. SIGINT, SIGTERM and SIGHUP, or Windows' console events, call `shutdown(false)`: **kept**.
- A thread's session path is read by a refresh, not by its event; the TypeScript's `busyThreads` read herdr itself. The Stop-order shutdown test refreshes after `set_state(Working, …)`: **kept**.
- The words are parsed by clap (0637): help and usage errors take clap's format. A word that is no command answers `error: unrecognized subcommand 'x'` with `Usage: jocasta [COMMAND]`, where the TypeScript printed `jocasta: unknown command` and its own usage. `project`, `thread` and `worktree` take raw trailing words, so their own usage texts still print. `--version setup` is a usage error: **kept** (0637).
- `--version` drops `bun …`: no Bun runtime is left: **kept**.
- An error that is neither a `UserError` nor a `DbError` prints `jocasta: <message>` and exits 1; the TypeScript crashed with a stack trace. `enter_surface` falls back to "I'm the last one" with a warning when its marker can't be written, where the TypeScript threw: **kept**.
- The runtime ends with `shutdown_background`, so a console read still waiting on stdin doesn't hold the process open; `on_terminal_close` is tokio's `ctrl_close` on Windows and SIGHUP on Unix: **kept**.
- `install_dir` requires `config/` beside `jocasta.exe`. The TypeScript told a source run by Bun's own exec path; a cargo build holds `jocasta.exe` in `target/<profile>/` but never `config/`, so it would have counted as an install and looked for a `config/` there: **fixed**.

### Wave 12 (the service tests, case for case)
- The relay listens on a free port in a test (`Some(0)`), and `start_rest` now hands the service the port it got; before, every pane's overlay named the default 47499. The TypeScript passes `relayPort: relay.port`: **fixed** (E2eKeys, E2eStartFinish).
- A project pane's check whose request is dropped (its client gone) goes on in its own task: a `LateCheck` guard spawns the late tail from its `Drop`, so the failure still reaches today and is owed to the next turn end, as the TypeScript's handler ran on past `request.signal`. A drop outside a runtime (the service ending) does nothing: **fixed**.
- Yesterday's worklog read the sessions with `try_lock` and wrote only its heading while another task held the index; `DayHooks::sessions_by_project_waiting` waits for the lock: **fixed**.
- One wall clock: `shared::clock::now_ms` replaces each module's own, and a test stands it still with `clock::testing::stand_still_at` (Bun's `setSystemTime`). The clock is the process's, as Bun's was: under `cargo test` a standing clock reaches the tests beside it, so a test keeps its values near the real time and holds a temp Jocasta: **kept**.
- Each test process's sandbox is `<temp>/jocasta-sandboxes/<pid>-<start>`, and one older than a day is removed. Named by the process id alone, a later nextest process met an earlier one's folder after Windows reused its id, and `make_repo`'s commit found nothing to commit: **fixed**. Bun's preload removed its sandbox in `afterAll`; a Rust test process has no such hook (a static never drops), so a sandbox stays until a later process's sweep, about 13 KB each: **kept**. The longer sandbox path took the fake omp host's Unix socket past `SUN_LEN` (about 100 bytes) on Linux, so that socket sits in the temp folder by its entry's name, as the fake herdr's does; the registry entry names it whole: **fixed**.
- Where the TypeScript tests replaced a method or spawned Bun: `FakeHerdr` has `on_close_workspace` (the wrapped `closeWorkspace`) and records a split's `direction` and `ratio`; a herdr that holds the boss's prompts is a wrapper in the test; a stand-in omp or shell is the test binary run again, or `ping`/`sleep`; a stand-in `gh` is a shim the test writes; a dropped request is a future dropped in `tokio::select!`. `askReport: undefined` is `ask_report: Some(true)` and `panePrompt: undefined` is the shipped prompt passed in, because the Rust helpers' `None` means off: **kept**.
- A client of the MCP endpoint sends rmcp's `initialize` before `tools/list`, which rmcp requires and Bun's server didn't; `find_review` gives the review's title, not an object. A test that replaced `herdr.respawn` or `stopServer` wraps the fake in `Hooked` (`helpers_procs.rs`), and `with_server_on` takes the herdr `withServer({ herdr })` took: **kept**.
- The day's delivery case: the boss's idle state, set before the first line, reaches the Rust service before that line goes in, so the line leaves the boss busy (CR-08) and the second normal line waits for the next idle; Bun handled that idle after the first line, which left the boss idle for the second. The test sets the boss idle once more, its only added step. A refused prompt (`agent_blocked`) is `FakeHerdr`'s `prompt_refused`, and the server passes `plan_settle_ms` on to the service as the TypeScript's `planSettleMs`: **kept**.

### Bench (0636), wave B1 (`bench/lib`, the evals core, A6's layer)
- `jocasta-bench` finds the repo at run time: the nearest folder above its executable that holds `bench/prices`, else the checkout it was built in. Bun's `import.meta.dir` was the script's own checkout, and the VM runs a binary built on the PC and copied into its checkout: **kept**.
- `jocasta-bench`'s tests run in the library's per-process sandbox from their first line (the `test-support` feature, on for the package's dev builds through a dependency on itself), as `test/preload.ts` put every Bun test there. A real `main()` (`jocasta`, `jocasta-bench`) turns the overlay off first, so a binary a test starts reads the environment it is given; without the feature the call is empty: **kept**.
- A6's layer is linked into the binary: `LayerUnavailableError`, `layerBuilt` and `loadBenchLayer` are gone, and the dry-run plan names `src/frontier/layer/bench.rs` without a "not runnable" branch. A layered trial's `.mcp.json` runs the `jocasta` beside the running executable with `mcp`, not Bun on `src/entry.ts`; the layer's endpoint goes through the service's `listen` and its host check; the layer's types (`LayerTrial`, `LayerSetup`, `BenchLayer`) live in the library: **kept**.
- Numbers are written as JavaScript writes them (`25`, `1e-7`, NaN as `null`; `js_number`), so files compare byte for byte. `exp` and `ln` may differ from V8's in the last bit, which can move a p-value's last digit: **kept**.
- Variants are validated by hand over the parsed YAML, in typebox's order and words (the unknown key first, then properties in schema order; `must be string`, `must be equal to constant`, `must not have fewer than 1 characters`), and `typebox` leaves with the TypeScript: **kept**.
- Error texts that came from V8 or Node come from serde or the OS: a bad JSON file (`SyntaxError`'s text), opencode's export failure, `read_accounts`; an error that isn't a usage error prints its message and exits 1 where Bun printed a stack trace. Session files sort lowercase-then-bytes, not by ICU's `localeCompare`. The OS release on Windows is parsed from `ver` and the CPU model read from the registry, the values libuv reads: **kept**.
- Where the TypeScript threw on a malformed input it now skips it: a feed entry without an object `message`, a JSON line that isn't an object (an empty row). `is_inside` drops the TypeScript's quirk that counted a `..foo` folder inside the root as outside: **kept**.
- The plan-review extension is still JavaScript that omp loads; its tests run the generated file on Bun with a small stand-in for omp's extension API, and its header names `evals/plan_review.rs`. The arms case "A6 fails loud without the layer's build" has no Rust twin: **kept**.
- An atomic write's temp file is named by the process id and a fresh suffix (`fsx::temp_beside`; `write_atomically` and the project overlay), not by the process id alone. The TypeScript's synchronous writes on one event loop never overlapped; the service's tasks run on several threads, and two writes of one file in the boss's start (`ensure_boss` writes the boss's files and `panes.json`, which the refresh tick writes too) shared a temp file, so one rename found it gone (ENOENT; seen as a Linux-only flake where `cargo test` runs every test in one process): **fixed**.

### Bench (0636), wave B2 (the evals runner, the main benchmark, compare, A5)
- A layered arm needs no build check: `layerBuilt()`, `layerMissingMessage` and the dry run's "NOT RUNNABLE" branch are gone, since the layer is linked into `jocasta-bench`: **kept**.
- The machine's name is `COMPUTERNAME` on Windows and the `hostname` command elsewhere, where Node read `os.hostname()`: **kept**.
- Tables print JavaScript's `toFixed` digits through a twin of it (`report.rs`, one for the runner and compare), and an unparsable date's local minute is the string `NaN`, as the TypeScript wrote: **kept**.
- The trial workers are one async task over `join_all` with a `Notify` for wake-ups; a trial runs on a blocking thread, as the TypeScript's worker thread ran it: **kept**.
- A missing tool is found on PATH before its command is built; the message is the TypeScript's (`<name> is not on PATH; …`). Seeds parse as `u64` and are cast to `u32`, as JavaScript's `>>> 0` wraps them; `text_under` scans in sorted order: **kept**.
- The usage errors name the new binary (`usage: jocasta-bench evals compare <runDirA> <runDirB>`); the evals' process tests are an integration test (`tests/evals_cli.rs`), since a binary's unit tests can't reach `CARGO_BIN_EXE_*`: **kept**.
- The evals' test fixtures are no longer Bun scripts: `jocasta-bench stand-in <mode>` (`omp`, `claude`, `sleeper`, `branch`, `hooks`, `hang`, `sleep`, `file-is`, `missing`), built only with `test-support`; the test binary can't stand in, since libtest prints its own lines on stdout: **kept**.
- A case's `case.yaml` is validated by hand (the variants' checker can't carry a check item's path prefix), and a YAML error is a plain error, not a usage error: **kept**.
- A case's setup and grading commands run through `cmd.exe /d /s /c` on Windows and `/bin/sh -c` elsewhere, through the product's `run_program` with its timeout; a failed `remove_tree` prints the OS's text, not the errno code: **kept**.
- `locale_compare` approximates ICU's `localeCompare` for task and case names (lowercase first, then bytes); `json_lines` skips a row that doesn't fit its type; `scan_leaks` matches case-insensitively by lowercasing, not by lookaround: **kept**.
- The dry run prints each arm's line after the order and before the quota pacer, as the TypeScript did; the first Rust cut skipped them: **fixed** (in the port, at the merge).
- The trial worker's case reads the stand-in's command (`stand-in file-is lib.txt fixed`) in `grade.log` where the TypeScript read its Bun fixture's `tests/check.ts`, and compares the trial root with the plain `--work` path, since Rust has no `realpath.native` that avoids Windows' `\\?\` prefix: **kept**.

### Bench (0636), wave B3 (H1's scripts and oracle, H2–H9, tokscale parity)
- H1's oracle is one TypeScript file on Bun, `bench/h1-guard/omp-verdicts.ts` run without arguments: one JSON request per line on stdin (`find`, `load`, `tokenize`, `judge-init`, `judge`, `exec-init`, `exec`), each answer one stdout line after `\u0001ORACLE `. Above its legacy section it imports only `node:*` and loads omp 18.20.2's own modules; the scripts around it (`omp-verdicts`, `git-guard-check`, `live-check`) are Rust. The git guard's refusals are added in Rust (the measured line, else the reimplementation on omp's tokens), so the oracle answers omp's approval alone; the rows are the same. Until the cutover the file keeps, under one marked comment, the exports the TypeScript bench and its tests import, and run with arguments it is still the old script: **kept**.
- `build-corpus` refuses an unknown `class` (exit 1), where the TypeScript left `expected` out: **fixed**.
- `REPO_ROOT` resolves with `pathkey::real_path`, not `canonicalize`, whose `\\?\` prefix `is_inside` never matched, so `--out` inside the repo went unrefused: **fixed**.
- Without `--omp` the oracle (Bun) starts before the corpus is read, so a missing Bun shows before the `--out`-exists check; the usage errors that came first still do. Plan and usage texts name `jocasta-bench` and the Rust modules (`git_guard.rs`, `evaluate.rs`); the `matcher` field of H1's rows still says `evaluate.ts`, so rows compare with earlier runs: **kept**.
- An ACP turn's exit code is null when the recorder kills omp, as in Node; omp's stderr in a live-check turn goes to null, since nothing read the TypeScript's `stderrTail`. The ACP stand-in (`fake_omp_acp`) is a test in the test binary, active only with `H1_FAKE_OMP_ACP`, printing a newline before each JSON line so libtest's prefix doesn't join the first. The recorder-extension and oracle tests need Bun (the oracle's also omp 18.20.2) and print "skipped" without them; the live tokscale test skips without `tokscale`: **kept**.
- H2–H9: an error that isn't a usage error prints its message and exits 1 where Bun printed a stack trace. H2, H4 and H6 build the herdr client through the sandbox, which never starts a server (BEN-18), where `connectHerdr` could start one. H3 kills by `proctree::kill_process` (one process, failing when it isn't alive, like Node's `process.kill`) and fails its run when a restart can't spawn, which the TypeScript didn't notice: **kept**.
- The detached `jocasta` H3 and H5 start goes through `ComSpec /d /s /c` with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`; std can't set Node's `windowsHide`, and whether a window shows is unobserved until the VM run. `installed_jocasta` finds `jocasta.EXE` on PATH with the extension's case from `PATHEXT`, where Bun gave `jocasta.exe`: **kept**.
- Temp folders are `bench/lib/temp.rs`'s `mkdtemp` in every script (`prefix` and six hex characters, not Node's alphabet); the machine name the VM checks compare is `sandbox::machine_name` (`COMPUTERNAME` on Windows), one function for H1's git guard check, H9 and the task benchmark: **kept**.
- H8: git and the merged tree's tests return `Result`; the Bun program is a parameter (the script passes `bun`); an empty `--threads` prints `-Infinity` as JavaScript does, and `peakWorkingSetMb` is `null` where JavaScript gave `-Infinity`; `--project` and `--repo` are read after the dry-run check; ordered lists replace the `Map`s: **kept**.
- The merge removed what the waves' split left: the `allow(dead_code)` and `allow(unused_imports)` the waves needed are gone, and so is what they hid (`FeedRecording::stop`, `KEEP_ALPHA`, `PriceTable.source`, whose test reads `pinned_source()`, `HarnessRun`'s unread status and timeout, `TrialRepo.branch`, the unread stderr tail); `grade_check` and `BootstrapOptions::resamples` are test-only. Every subcommand takes one `ScriptArgs` and ends through `run_script` or `finish_script`: **kept**.
- A `thread_status` call joined a read of a thread's files that the refresh tick had started before a change, and answered with the files as they were (0456's overlaps; seen once under load in B3's gate, in both builds' design). A call now starts its own read, which later callers join, and a read that ends after a later one never replaces what that one cached: **fixed**.

### The cutover (0628)
- A load check (the e2e tests looped beside a release build) found two races the port added. Two task-file writes could each read the thread under its lock and save outside it, so the older read could land last and lose a `## Report`; the snapshot and the save now hold `StatusViews::task_writing`, and so does the finish's close (the lock rule's one exception, above). A plan `thread_send` found while the turn-end change of the same pane was still being handled waited for the next refresh tick; the pane's queue now notes it, and the plan is checked again when that change ends, a reopened omp's Plan Review shown again then as the tick does (0494): **fixed**, each with its test. B1's test then passed 600 runs of 600 under load; one earlier failure of it, in its first waits (1 of 160 runs, before the fix), was not seen again and stays unexplained.
- The number formatter had four copies; one `protocol::js_number_text` serves them all, and `omp_settings::js_number` keeps only its `-0` case. `evals::args::js_number` is `number_of`, the name its callers used: **fixed**.
- The texts that said to run `bun run build` (whiteboard not built, the canvas's trouble line) say `mise run build`: **fixed**.
- `test/entry.test.ts` run against the Rust binary before the deletes: 3 pass, and the 2 that fail are recorded differences (`--version` names no Bun; an unknown word gets clap's error, 0637). `tests/entry.rs` covers both: **kept**.
- The canvas tests serve the page from a Bun stand-in server, not the Rust service, since `jocasta service` needs a real herdr and `FakeHerdr` is test-only. `evals/cases/**` stays TypeScript (the eval tasks' own projects). Rust comments that name a deleted `.ts` twin stay as history: **kept**.
- The build compiles the host's target only (`mise run build`: cargo, then `scripts/build.ts` packages `jocasta`, whiteboard, Node, hunk and `config/`); `jocasta-bench` lands in `dist/<target>/bench/`, out of the zip. The zip still carries `editor.worker.js`, which the binary embeds and serves itself; dropping it changes the install layout: **kept**.
- The two herdr live smokes are Rust tests, `live_smoke_*` in `herdr_tests.rs`, run by `mise run smoke` (`JOCASTA_HERDR_SMOKE=1`) in a throwaway session: **kept**.
- A live smoke on the installed release (`install.ps1` from the local build into a sandbox, its own herdr session and port) passed every step: setup and diagnose, the service, the boss's tools and the CLI on a throwaway project and thread, the pages with their policy and the worker, and Stop everything with no process left. `install.ps1` still writes `autostart` and `appData`, which uninstall reads to clean what the companion left: **kept**. Stop everything counted as working a thread `thread list` showed idle, whose omp had no login; the close question counts herdr's state as well as the shown one (0434), and this wasn't traced further. `at_work` is the TypeScript's `atWork` line for line (`state` or `shown` working, not awaiting), so this is 0434's rule and not the port's: **open**, for his v1 review.
- The gate run of 2026-11-18 failed one test every time: `e2e_0298_a_hand_commit_in_quiet_hours…` waited for a held line that never reached the boss. The test worlds read the real disk; with C: at 8% free, every day tick raised the urgent "Disk almost full" line, which kept the boss busy ahead of the held one. `with_test_defaults` and `test_context` now pin the disk at 50% free, as `day_tests` already did; a test of the disk passes its own `disk_free`: **fixed**.

## The service's shape

For the agents that port the service's parts (`finish`, `status`, `thread_start`, …). `context.rs` is the TypeScript's `ServiceContext` (SVC-15); `service.rs` wires it. `events.rs` is the first part ported on this shape.
- **A part** is a struct in its own module file, `pub struct X { pub link: Link, <its own state> }` with `pub fn new(link: Link) -> Self`; the stubs declare exactly that. Read options when a method runs (`self.link.ctx().options`), not in `new`: the context doesn't exist yet. If `new` needs an argument, add it and change the one line in `ServiceContext::new`.
- **The context** is an `Arc<ServiceContext>` built once: `ServiceContext::new(&link, catalog, options, day)` builds every part, then points the `Link` (a weak, `OnceLock`-set) at itself. That is the late initialisation: whatever needs the context while it is built (a part, `Day`'s hooks, a `Places` hook) takes a `Link` clone first. Things built after it (`Refresher`, `Tools`, `Keys`, `PushGate`, the watcher) are not fields; they take the `Arc<ServiceContext>`.
- **Reaching a sibling:** `let c = self.link.ctx();` then `c.events.tell(..).await`. Never store another part or the `Arc` in a field (a cycle), and a spawned task clones `c`, not `self`. A part method running means the context is alive; `ctx()` can't fail there.
- **State, TypeScript field by field:** `stopping`, `warmed` are atomics (`stopping()`, `set_stopping()`, `warmed()`, `set_warmed()`); `boss` is `ctx.boss() -> Option<BossRef>` (`Shared<BossPane>`); `finishing` is `ctx.finishing(ws)` / `set_finishing` / `clear_finishing`; `calls` is `ctx.calls` (`run(fut).await`, `spawn(fut)`, `settled_within(ms)`: `stop`'s wait); `warn` is `ctx.warn(err)` (silent once stopping); the registry is `ctx.threads` and a thread is a `ThreadRef` (`Shared<Thread>`): `t.with(|t| ..)` reads or changes it, `t.same(&u)` and `ctx.threads.current(&t)` compare identity, as `===` did. `ctx.places`, `launched`, `day`, `allow` take `&self`; `ctx.sessions` is a tokio `Mutex` (its methods are `&mut self` and `warm` and `prepare` run over awaits).
- **Locks.** A part's own mutable state sits in one private `State` behind a std `Mutex` (see `Events::st`). `Shared::with` takes a synchronous closure, so no guard can outlive it.
  - **Rule:** no std guard across an `.await` (clippy's `await_holding_lock` is denied here), and no lock of any kind while another part, a callback (`Holds`, a hook) or a `Shared::with` of another value is called: copy what is needed out, drop the guard, then call. A `with` closure only reads or changes its value.
  - The one tokio guard is `ctx.sessions`: held across `warm` and `prepare` only, never across a part's method, and a synchronous caller (a `DayHooks` method) uses `try_lock` and takes a busy index as "nothing new yet".
  - The one exception to the rule is `StatusViews::task_writing`, held from a task file's snapshot (a `with` of its thread) to its write, so an earlier snapshot never lands over a later one. It is only ever taken outermost, by `write_task` and by the finish's close, and nothing inside a `with` takes it.
  - Work that starts without being awaited goes through `ctx.calls.spawn` (or `Events`' own `Calls`), so `stop` waits for it.
- **Not `Send`:** `sync_catalog` holds a connection across awaits; `Places::sync` runs it in `spawn_blocking`. `Places` was changed for this (it owns the `SharedCatalog`, its hooks are `Send + Sync`, every method takes `&self`); its behaviour is as before.
- **A part's test** builds its world with `context::test_context(|options| ..)` (in `context_tests.rs`, `#[cfg(test)]`; inside a `#[tokio::test]`): a temp Jocasta (its data dir holds the catalog), a `FakeHerdr`, a day with no pollers and no hooks, every part. It returns `TestContext { ctx, herdr, sent, .. }`: `herdr.of("prompt")` is what herdr was told, `sent` what the clients got, `with_boss(state)` and `with_thread(pane, title)` make a boss pane and a registered thread. See `events_tests.rs`.
