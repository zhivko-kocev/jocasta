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
| File watching | notify 8.2.2 (7 d, 10) | |
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
