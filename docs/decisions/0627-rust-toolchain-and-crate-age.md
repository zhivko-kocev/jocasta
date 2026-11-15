# 0627 Rust 1.100.0 through mise; crates follow the 7-day rule

- Status: v1
- Kind: process, product
- Phase/slice: program
- Date: 2026-11-15

## Question
Which Rust toolchain does the port of 0625 use? And does 0614's 7-day package rule cover crates?

## Facts
Verified 2026-11-15 (`docs/research/rust-port.md`).
- **On the PC:** rustc and cargo 1.93.1, rustup 1.28.2. Rust via rustup was approved in 0385.
- **Current stable:** Rust 1.100.0, released 2026-11-12.
- **Package age.** Cargo 1.100 adds a minimum package age: `[registry] global-min-publish-age = "7 days"` (RFC 3923). The resolver skips versions younger than that unless they are already in `Cargo.lock`. Cargo 1.93 ignores the key without a word.
- **mise** installs Rust through rustup with a profile and components, and locks the version in `mise.lock`.
- **herdr** pins 1.96.1.
- **Two crates need a newer rustc than 1.93.1:** sqlx 0.9 (1.94) and sysinfo 0.40 (1.95).

## Options
| Option | For | Against |
|---|---|---|
| 1.100.0 pinned in `mise.toml` (minimal profile, clippy, rustfmt) and the 7-day age in `.cargo/config.toml` | one pinned dev entry, the same supply-chain rule as Bun | a toolchain download |
| Stay on the installed 1.93.1 | no download | no age rule for crates; some crates need a newer rustc |
| 1.96.1, as herdr | a precedent | a download, and no age rule |

## Recommendation
1.100.0 through mise, with the age rule.

## Choice
- **Rust 1.100.0 is pinned in `mise.toml`,** with the minimal profile plus clippy and rustfmt, and locked in `mise.lock`.
- **`.cargo/config.toml` sets `[registry] global-min-publish-age = "7 days"`.**
- **0614's rule covers crates:** a crate version must be 7 days old to enter `Cargo.lock`.

## Reason
His answer: `1.100-mise-cooldown`.

## Consequences
- This record amends 0476 (mise pins Rust beside Bun and Node) and 0614 (the 7-day rule covers crates).
- **Install approved:** Rust 1.100.0 through mise, using the rustup already on the PC.
- **A Rust bump** changes `mise.toml` and `mise.lock` together, as a Bun bump does.
- AGENTS.md's list of approved installs names it.
