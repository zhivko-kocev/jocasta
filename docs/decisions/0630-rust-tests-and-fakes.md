# 0630 The tests are ported with their modules, and the fakes are rewritten in Rust

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-15

## Question
How are the Rust build's tests written, and what happens to the fakes?

## Facts
Verified 2026-11-15.
- **103 test files; 99 of them import `src/` internals.**
- **The fakes are separate TypeScript programs and sockets:** `fake-herdr`, `fake-mise`, `fake-mise-verify`, `fake-hunk`, `fake-gh`, `fake-omp`, `fake-hosts`, and the evals' CLI fakes. They encode verified quirks; for example, `fake-herdr` drops requests the way herdr 0.9.5 does (0517).
- **`test/preload.ts`** gives every test process its own Jocasta and omp folders.

## Options
| Option | For | Against |
|---|---|---|
| Each module's tests ported with it; black-box tests drive the binary against the TypeScript fakes, run with Bun | the verified fakes stay as they are | the Rust tests need Bun on PATH |
| The same, with the fakes rewritten in Rust | one language in the tests | the fakes are re-encoded, with a risk of drifting from the verified behaviour |
| Black-box tests only | less test code | edge cases now covered by unit tests lose their coverage |

## Recommendation
Port the tests with their modules and keep the TypeScript fakes.

## Choice
- **Each module's tests are ported with it, case for case,** as Rust unit tests.
- **Black-box tests in `tests/`** drive the `jocasta` binary.
- **The fakes are rewritten in Rust,** keeping each quirk they encode.
- **`mise run check`** gains `cargo fmt --check`, `cargo clippy` with warnings as errors, and `cargo test`.

## Reason
His answer: `all-rust`.

## Consequences
- This record amends 0476 (the check task runs the Rust checks).
- **Test isolation.** Every Rust test gets its own Jocasta and omp folders, as `test/preload.ts` gives today.
- **Tests that stay TypeScript:** the canvas page's own tests stay with the page (0625), and so does the test of `config/pane-check-extension.ts`.
- **At the cutover,** the TypeScript tests of ported modules are deleted together with their modules.
