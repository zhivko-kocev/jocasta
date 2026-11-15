# 0639 The port's remaining crates and tools

- Status: v1
- Kind: product, process
- Phase/slice: program
- Date: 2026-11-15

## Question
The port's remaining crates and tools, asked as a tick-list (0089). Anything he unticked would have come back as its own question.

## Facts
Verified 2026-11-15. Crate counts are transitive, on Windows, on top of the port's base.
- **rusqlite 0.42 with `bundled`** adds 7 crates. It compiles SQLite with FTS5 and uses libsqlite3-sys 0.40, which is past the RUSTSEC-2026-0459 fix (0.39). The alternative, sqlx, adds 94 crates, is async-only, and can't share one SQLite with rusqlite.
- **windows-sys 0.61 covers the Win32 calls** Jocasta makes through `bun:ffi` today:
  - the Right Ctrl hook;
  - Credential Manager;
  - Toolhelp;
  - Job Objects.
- **process-wrap 10** ends a whole child tree, through Job Objects on Windows and process groups elsewhere.
- **notify 8** watches files, with `ReadDirectoryChangesW` on Windows.
- **Together,** windows-sys, process-wrap and notify add 18 crates.
- **The canvas page and its Monaco worker** are bundled with Bun today (`Bun.build`), at build time or at the first request in a source run.
- **cargo-deny 0.20** checks advisories and licences. It is the Rust counterpart of `bun audit` (0614).

## Options
Each item was offered ticked:
- rusqlite with `bundled`;
- serde and serde_json;
- windows-sys, with process identity hand-written as today (Toolhelp, process times, the PEB cwd, `/proc`, `ps`) and no sysinfo;
- process-wrap;
- notify;
- Bun as the build tool for the canvas page;
- cargo-deny through mise.

## Recommendation
All seven.

## Choice
- **Crates:** rusqlite 0.42 with `bundled`, serde and serde_json, windows-sys 0.61, process-wrap 10, and notify 8.
- **Process identity** is hand-written, as today; sysinfo is not used.
- **Bun bundles the canvas page and the Monaco worker** at build time, and the Rust binary embeds them.
- **cargo-deny 0.20** is installed through mise and runs in `mise run check`.

## Reason
His answer: all seven ticked.

## Consequences
- This record amends 0476 (the check runs cargo-deny) and 0614 (cargo-deny is the crates' audit beside `bun audit`).
- **Install approved:** cargo-deny 0.20 through mise.
- **Bun stays a development tool** after the cutover. It bundles the canvas page and runs H1's oracle, the scorecard and the omp² watch.
