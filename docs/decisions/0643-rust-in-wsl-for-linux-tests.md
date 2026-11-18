# 0643 Rust 1.100.0 in his WSL Ubuntu for the port's Linux test runs

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-15

## Question
0641's Unix half (process groups and their kill) has never been compiled, because every check ran on Windows. How is the port's Unix side tested?

## Facts
Verified 2026-11-15.
- **His WSL Ubuntu has a rustup** with one toolchain, `nightly-2026-11-08`, and no default. It also has gcc and Bun 1.4.3, which 0572 approved for Linux test runs and `build.rs` needs.
- **CI runs no cargo yet,** and it needs a push.
- **0627 pins Rust 1.100.0.**

## Options
- **Install Rust 1.100.0 in WSL through its rustup, for Linux test runs only.**
  - Pros: the pinned compiler; the full suite runs at each wave's integration.
  - Cons: one install of about 600 MB.
- **Use the nightly already there.**
  - Pros: no install.
  - Cons: a different compiler from the pin.
- **Leave it to CI after a push.**
  - Pros: covers macOS too.
  - Cons: needs a push and a CI change; faults show up late.

## Recommendation
Install 1.100.0 in WSL.

## Choice
Rust 1.100.0, with clippy and rustfmt, is installed in his WSL Ubuntu through its rustup. It is used only for the port's Linux test runs.

## Reason
His answer: `install-1.100`.

## Consequences
- **Install approved:** Rust 1.100.0 in his WSL Ubuntu, beside 0572's Bun 1.4.3. rustup made it the default there, because there was no default before.
- **The Linux suite runs** at each wave's integration, from a copy of the repo inside WSL.
