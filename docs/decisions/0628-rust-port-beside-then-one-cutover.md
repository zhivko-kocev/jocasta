# 0628 The Rust build grows beside the TypeScript one; one cutover at the end

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-15

## Question
How does the port of 0625 land: beside the TypeScript build with one cutover, piece by piece in a shipping product, or by replacing in place? And in what order?

## Facts
Verified 2026-11-15.
- **The benchmarks need the whole Rust build** (0625).
- **The TypeScript build is the contract.** Its tests and plan appendix I are what the port must match (0625).
- **He doesn't use Jocasta at the moment** (his first answer, below), so no daily install has to be protected.
- **Size:** 27,976 lines of TypeScript in `src/`, not counting the canvas page.

## Options
First form:

| Option | For | Against |
|---|---|---|
| Beside the TypeScript build, then one cutover | his daily tool never runs half-ported | no live use of the Rust code until the end |
| Piece by piece: the TypeScript `jocasta` hands each ported word to the Rust binary | live use early | both runtimes in one install; seams between two languages over one SQLite file and token file |
| Beside, with the service first | the largest memory gain is measured first | the hardest piece comes first |

After his answer, second form:

| Option | For | Against |
|---|---|---|
| Beside, then one cutover | a working build shows how every ported piece behaved before, and a stop midway leaves a working product | two code bases until the cutover |
| Replace in place, module by module | no duplication | no working product until the last piece |

## Recommendation
Beside, then one cutover.

## Choice
- **The Rust build grows beside the TypeScript build** in the same repo. The TypeScript build stays runnable and checked until the cutover.
- **Order:**
  1. shared code;
  2. `jocasta mcp` and the internal roles `push-hook` and `pane-check`;
  3. the CLI words and setup;
  4. the service;
  5. `today` and `ask` with the plugin role.
- **One cutover.** When the Rust build passes its tests, the black-box tests and a live smoke, the TypeScript product code and its tests are deleted, and Jocasta installs from the Rust build.

## Reason
His answers:
- First form: "i dont use jocasta now at the moment".
- Second form: `beside-then-one-cutover`.

## Consequences
- **`mise run check` checks both builds** until the cutover.
- **The cutover is a slice of its own.** It updates plan appendix I, AGENTS.md's package map and the install scripts.
- **Plan appendix I** changes with each slice that changes a seam (0027).
