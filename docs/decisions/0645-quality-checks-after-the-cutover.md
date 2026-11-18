# 0645 The quality checks after the cutover

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-16

## Question
At the cutover (0628) the TypeScript product code goes, and with it what 0610's syntax-tree checks, 0614's knip and 0608's docs and config checks measure. 0636 keeps the scorecard on Bun; nothing says what the others become.

## Facts
Verified 2026-11-16.
- **The ceilings already have Rust twins.** `clippy.toml` holds 0610's limits (5 parameters, 120 lines a function, complexity 40), and `Cargo.toml` denies those lints with all of clippy's defaults.
- **0610's syntax-tree checks** (`test/quality`) flag three smells over `src`: flag parameters, functions that only forward, and exported items without a comment. They fail on new cases only; today's are the baseline. Of the three, rustc has `missing_docs` (about 105 public Rust items have no comment today) and clippy has `fn_params_excessive_bools`, which is narrower than the flag check. No lint finds a function that only forwards.
- **`syn` 3.0.10 is already in the lock,** built for the derive macros. A test on it is a direct dev-dependency with no new download.
- **knip's job in Rust.** Dead code is already caught: clippy's `all`, denied, includes rustc's dead-code warnings. For unused dependencies, rustc's `unused_crate_dependencies` lint, in one package with a library, two binaries and their tests, reports in each target the crates only the others use.
- **cargo-machete** finds unused dependencies by scanning the sources. 0.9.2 (2026-04-15) is its newest release past the 7-day rule (0627); 0.10.0 and 0.10.1 came out on 2026-11-14 and 11-15. 0.9.2 publishes a Windows build with its SHA-256, which mise installs through `github:bnjbvr/cargo-machete`.
- **The docs and config checks** (`test/docs`, `test/config`) read docs, config and source files as text. Bun stays a development tool after the cutover (0639).

## Options
- **0610's checks.**
  - A Rust test on `syn` with all three checks and today's cases as the baseline. Pros: the same three checks. Cons: a direct dev-dependency and about 200 lines.
  - Built-in lints only. Pros: nothing to maintain. Cons: the forwarding check goes, and the flag check narrows.
  - Dropped. Cons: 0610's checks end.
- **Unused dependencies.**
  - cargo-machete through mise. Pros: purpose-built. Cons: one more pinned tool.
  - rustc's `unused_crate_dependencies`. Cons: a `use x as _` line per crate a target doesn't use.
- **Docs and config checks.**
  - Stay Bun tests, pointed at the Rust sources. Pros: a small change. Cons: the check keeps a Bun test step.
  - Ported to Rust tests. Cons: about 400 lines rewritten for no new check.

## Recommendation
The `syn` test, cargo-machete, and the docs and config checks on Bun.

## Choice
- **0610's three syntax-tree checks become a Rust test on `syn`.** It covers flag parameters, functions that only forward, and public items without a doc comment. Today's cases are its baseline, and it fails on new ones only, as before.
- **cargo-machete 0.9.2,** through mise (`github:bnjbvr/cargo-machete`), checks for unused dependencies in `mise run check`. It takes knip's place for Rust.
- **The docs and config checks stay Bun tests,** pointed at the Rust sources.
- **The scorecard (0616)** stays a Bun script (0636). It measures clippy's counts at the reporting thresholds, jscpd over `src` and cargo-deny's advisories in place of `bun audit`.

## Reason
His answer: `recommended`, after he asked which option is best for each and was shown the recommendations with their reasons.

## Consequences
- Amends 0610 and 0614: what they check moves to the Rust build at the cutover. Biome, knip and the TypeScript tree checks keep checking the canvas page.
- **Crate approved:** `syn` as a direct dev-dependency, at the version in the lock.
- **Install approved:** cargo-machete 0.9.2 through mise, under the 7-day rule.
- Built in the cutover slice (0628).
