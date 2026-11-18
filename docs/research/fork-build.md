# The fork's build: the ledger (2026-11-18 on)

The build of `docs/research/fork-plan.md`, run unattended by his word of 2026-11-18 ("reccomended without questions i review when done"). Every visible choice made while building is listed here as built (0363), with the reason; the records 0649+ are written from this ledger and `docs/research/two-plugins.md` in the last phase.

## Choices as built

| # | Phase | Choice | Why |
|---|---|---|---|
| 1 | 1 | `git subtree add --squash`: herdr v0.9.5 enters as one squashed commit, not with upstream's history. Upstream releases still come in with `git subtree pull --squash`. | AGENTS.md's pre-push rule: every author and committer address is the repo identity. Upstream's history would put herdr's authors in `git log` and stop every push, so CI on the three OSes could not prove each phase. The plan's default (no push until he decides) would leave macOS unproven all night. |
| 2 | 1 | Removed from `crates/herdr`: herdr's agent and contributor files, `build.rs`, `.github/`, Nix, `justfile`, its own toolchain, `.cargo/` and `Cargo.lock`, `SPONSORS.md`, the Chinese README and the README images. `crates/herdr/FORK.md` names what was removed and changed (Apache-2.0 §4(b)) and how an upstream release comes in. | The workspace root holds the toolchain (mise's Rust 1.100.0, 0627), the lock and CI; herdr's `build.rs` only printed its contributor policy. Two lock files or two toolchains would drift. |
| 3 | 1 | One Cargo workspace: the root package plus `crates/herdr` and its `crates/ghostty-vt`, `default-members = ["."]`, `portable-pty` patched to herdr's vendored copy at the root. | Cargo allows patches only at the root; default members keep `cargo fmt`, clippy and nextest on Jocasta's own code until phase 3 makes herdr a library of the one binary. |
| 4 | 1 | The MSVC static C runtime (`+crt-static`) for Windows builds of the whole workspace, in `.cargo/config.toml`. | herdr's own `.cargo/config.toml` set it so its binary runs without the Visual C++ redistributable; Cargo reads that file only at the workspace root, and Jocasta's binary is the same binary from phase 3 on. |
| 5 | 1 | Zig 0.16.0 through mise (`zig = "0.16.0"`), its `mise.lock` entry written by hand from ziglang.org's official index (`download/index.json`, release 2026-04-13), the same shape and checksums `mise lock` writes. | ghostty-vt's `build.rs` runs `zig build`; CI installs tools with `mise install --locked`, which refuses a tool missing from the lock, and `mise lock` needs his `mise trust` of the edited `mise.toml`, which is his call. The checksums are the index's own `shasum`s. |
| 6 | 1 | `CARGO_BUILD_JOBS = "4"` in `mise.toml`'s `[env]`. | His PC's memory with herdr's build and Zig beside his work: four jobs built herdr in 3 m 23 s and stayed within it. |
| 7 | 1 | The lock re-resolved once for the workspace: herdr's crates take the newest semver-compatible versions beside Jocasta's (one `ratatui` 0.30.2, one `tokio` 1.54), except `interprocess`, held at 2.4.4, the version upstream's lock tested. | One version per crate where semver allows (the root already holds ratatui 0.30.2). herdr's server and client rest on `interprocess`'s local sockets and named pipes, and its four releases since 2.4.4 were never run under herdr's suite; a bump waits for an upstream release that takes it. |
| 8 | 1 | Biome skips `crates/`. | herdr's TypeScript (the docs scripts, the agent-state assets) follows herdr's style, not the repo's lint rules (plan finding 27). |
| 9 | 1 | `[profile.dev]` `debug = "line-tables-only"`, and no debug info for dependencies. | The plan's rule 0: herdr's size makes full debug info the heaviest part of a link on his PC; panics and backtraces still name lines. |
| 10 | 1 | CI: `cargo build -p herdr --locked` on the three OSes, and a 90-minute limit (45 before). herdr's own suite joins CI in phase 2, once the trim removes its three tests that fail on Windows upstream too. | Proves Zig and the link on macOS, which this PC cannot; the fork's build from scratch outgrows 45 minutes (plan finding 33). |
| 11 | 1 | herdr's test `render_keeps_halfwidth_katakana_voiced_tail_empty` checks what the host terminal is sent (no write over the wide tail) instead of the test backend's copy. | It is the one test the workspace's newer ratatui (0.30.2, which Jocasta already used) changes: 0.30.2 counts the halfwidth voiced mark's cell and skips the tail, which still keeps the host terminal from overwriting the kana, the test's own purpose. Upstream's lock passes it with 0.30.0. |

## For him at the review

The plan's section 6 items, each with the default taken, are repeated here as they are met.

1. **Upstream history against the identity rule** (plan 6.1): taken as a squashed subtree (choice 1), so every push and CI run went ahead. To keep herdr's history instead, the rule would have to exempt the subtree's commits.
2. **herdr's crates in the lock** (plan 6.9): 33 direct crates (27, plus 6 per platform); the lock grows from 389 to 465 packages, the rest already shared with Jocasta. cargo-deny passes them as they are (no new licence, advisory or source exception). CI's limit is 90 minutes.
3. **`mise.lock`'s Zig entry is hand-written** (choice 5; Zig through mise is his approval of 2026-11-18). Run `mise trust mise.toml` and then `mise lock` once; it should leave `mise.lock` unchanged.
