# 0644 cargo-nextest runs the Rust tests

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-15

## Question
The Rust suite went from about 65 s to 363–401 s on Windows with wave 8. How does it get fast again?

## Facts
Verified 2026-11-15.
- **One lock serialises the suite.** Every test with a temporary Jocasta home holds a process-wide environment lock for its whole run, because the code finds its folders through environment variables. So the git-heavy service tests run one at a time; their run times add up to 4,082 s, squeezed into about 300 s of wall time.
- **More of these tests are coming.** The last waves port about 6,000 lines of `service-*` tests verbatim, which would push `mise run check` past 15 minutes.
- **cargo-nextest runs each test in its own process,** so a process-wide lock never makes another test wait. 0.9.151 (2026-11-03) is the newest release past the 7-day rule (0627); mise installs it as `aqua:nextest-rs/nextest/cargo-nextest`.

## Options
- **cargo-nextest through mise, run by `mise run check`.**
  - Pros: no code change; an expected 60–90 s.
  - Cons: one new tool and a second runner beside `cargo test`.
- **Folders passed in, not read from the environment.**
  - Pros: tests run in parallel in one process, with no new tool.
  - Cons: it touches dozens of modules while the port keeps changing.
- **Leave it until the port is done.**
  - Cons: 7–15 minutes per wave's check.

## Recommendation
cargo-nextest.

## Choice
cargo-nextest 0.9.151, through mise, runs the Rust tests in `mise run check`, in place of `cargo test`.

## Reason
His answer: `nextest`.

## Consequences
- Amends 0476: the check's Rust tests run through cargo-nextest.
- **Install approved:** cargo-nextest 0.9.151 through mise, under the 7-day rule.
- **WSL keeps `cargo test`** for 0643's Linux runs, where the suite takes about 100 s.
