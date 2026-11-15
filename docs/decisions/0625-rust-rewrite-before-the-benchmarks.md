# 0625 Everything Jocasta owns is rewritten in Rust, before the benchmarks

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
Should Jocasta be rewritten in Rust? If so, how much of it, and when: before or after the VM benchmarks?

## Facts
Verified 2026-11-14 and 11-15 (`docs/research/state-of-the-art.md` section 15).

**Size and resource use**
- `jocasta.exe` is 118.9 MB. 97% of that is the Bun runtime; Jocasta's own code and assets are about 3.6 MB.
- Idle floor: a Rust process uses 4.3 MB working set, a Bun-compiled one 17.0 MB.
- Start time: `jocasta --help` takes 35 ms; a Rust hello world takes 18 ms.
- omp, not Jocasta, dominates memory and CPU. One omp tree uses about 2.8 GB working set and 574 CPU-seconds in 51 minutes. A pane's `jocasta mcp` uses 21.6 MB, and a sandbox Jocasta stack sums to about 490 MB.

**Effect on the program's targets**
- No option changes tokens (T1) or replaces a model step (T2).
- A rewrite before the benchmarks changes the artifact they measure.

**Scope of a port**
- About 19,200 lines for the service, the panes and the MCP server; 25,900 with `shared/`; 28,700 with setup.
- 94 of 113 test files import `src/` internals, so they do not carry over as they are.
- The canvas host page (1,963 lines) runs in his browser, so it can only be TypeScript or WASM.
- The benchmark's H1 oracle loads omp's TypeScript (`bench/h1-guard/omp-verdicts.ts`).

**What is not Jocasta's**
- whiteboard (a submodule at tag v0.5.2, with its Node host and desktop app), hunk, herdr and omp are external.

**Tooling and libraries**
- rustc and cargo 1.93.1 are on the PC. Rust via rustup was approved in 0385.
- `today` is drawn with OpenTUI (0551). A Rust terminal UI library would be a new dependency.

## Options
First question:

| Option | For | Against |
|---|---|---|
| RS1 No rewrite (recommended) | every test and benchmark stays valid | about 15 MB per pane and the 119 MB exe stay |
| RS1 now, RS2 or RS3 looked at again after v1 | the benchmark artifact stays stable | a second decision later |
| RS2 Rust for `jocasta mcp` only | about 15 MB and 15 ms less per pane | a second toolchain for 0.5% of an omp tree |
| RS3 Rust for the service, the panes and the plugin roles | the stack goes from about 490 MB to tens of MB, and the exe shrinks | 19–26k lines ported, tests rewritten, benchmarks run again |

Follow-up, when:

| Option | For | Against |
|---|---|---|
| After the benchmarks and his v1 review (recommended) | the benchmarks measure what was designed and tuned | Bun's memory and the 119 MB exe stay through v1 |
| Now, before the VM benchmarks | the benchmarks measure the Rust build | weeks of porting before any benchmark, and every test rewritten first |
| In a branch alongside the benchmarks | the benchmarks run on today's build | two code bases kept in step |

Follow-up, what stays TypeScript:

| Option | For | Against |
|---|---|---|
| The canvas host page, plus `bench/`, `evals/` and `scripts/` (recommended) | the shipped product becomes Rust; the harnesses that load omp's TypeScript stay as they are | two languages in the repo |
| The canvas host page only | one language outside the browser | H1's oracle would need a bridge to omp's TypeScript guard |
| Decided in the rewrite's own grilling | decided with the port's facts in hand | left open now |

## Recommendation
RS1. If a rewrite: after the benchmarks, with the canvas page and the dev harnesses kept in TypeScript.

## Choice
- **Everything Jocasta owns is rewritten in Rust:**
  - the service;
  - the `today` pane and the `ask` popup;
  - `jocasta mcp`;
  - the CLI and its internal roles;
  - setup.
- **whiteboard, hunk, herdr and omp are external** and stay as they are.
- **The canvas host page stays TypeScript,** because it runs in the browser.
- **The rewrite comes before the VM benchmarks,** so the benchmarks measure the Rust build.
- **`bench/`, `evals/` and `scripts/`:** they stay TypeScript if they need to (H1's oracle loads omp's TypeScript); otherwise they are ported too. This is decided when the rewrite starts.

## Reason
His answers:
- On the scope: "everything that we can/we own rewrite in rust, whiteboard is an external i guess?"
- On the timing: `before-benchmarks`.
- On what stays TypeScript: "if needed the first one if not the second one decide when started".

## Consequences
- Amends 0036 and 0037 (the product's toolchain and language), 0551 (`today`'s drawing library is chosen again), and 0533 and 0599 (the benchmarks run on the Rust build, after the rewrite).
- **Decisions before any Rust code:** the rewrite's own decisions go through grilling first (0086, 0005), one record each. Among them:
  - crates, including the async runtime, the HTTP server, YAML, SQLite, iCalendar and the terminal UI;
  - the crate and package layout;
  - the order of porting;
  - the tests' strategy;
  - what of `bench/`, `evals/` and `scripts/` is ported.
- **Installs:** each new crate is an install he approves; whether 0614's 7-day rule covers crates is asked in the rewrite's grilling.
- **The tuning program's TypeScript builds stay** the behaviour the port must match. Its tests and the plan's appendix I are the contract.
- **Two files stay TypeScript by what loads them,** as the canvas page does: the canvas host page (the browser) and `config/pane-check-extension.ts` (omp loads it with `-e`, 0617). Noted after his answer, from the facts; not asked.
