# 0657 The VM's benchmarks after the VM

- Status: v1
- Kind: benchmark, process
- Phase/slice: fork build, review
- Date: 2026-11-23

## Question
0652 removed the VM and left the scripts tied to it without a machine (`fork-build.md` For him 20), A6 refused on his omp folder (For him 19), and H1's oracle without omp 19.1.0's sources (For him 15, 23). Where does each run, and what is installed for them?

## Facts
Verified 2026-11-23 (`bench/README.md` "Where they run"):
- `h6-memory`, `h8-parallel` and `h1-guard live-check` make model turns and act on a sandbox Jocasta, as H2–H5 already do on his PC; nothing in them checks the machine.
- `h9-footprint` and `h1-guard git-guard-check` refuse any machine but `JOCASTA-CLEAN` (`bench/lib/sandbox.rs`): H9 measures what an install leaves on a clean machine, and git-guard-check runs the corpus's git lines. Their VM results, from the TypeScript build, are in `D:/thesis/data/vm-results/`.
- opencode 1.18.22 (`opencode-ai` on npm; A7, and H5/H6's competitor) and tokscale 4.21.0 (`tokscale-parity`, 0538) were installed in the VM only.
- An omp profile `<name>` keeps its own folder, `~/.omp/profiles/<name>/agent`, with its own `agent.db`: settings, sign-ins and `mcp.json` apart from his default folder (pi-utils `dirs.ts`).
- His global Bun package `@oh-my-pi/pi-coding-agent` is 18.20.2; `test/config/guard.test.ts` skips its 6 oracle cases while it differs from the pin, 19.1.0.

## Options
- H6, H8 and live-check: **his PC in a sandbox Jocasta**, or not rerun.
- H9 and git-guard-check: **not rerun, the VM's results reported as the TypeScript build's**; Windows Sandbox (a Windows feature to turn on and both scripts adapted); or a new VM.
- opencode and tokscale: **installed on his PC for the benchmarks only**; or A7 and tokscale-parity dropped; or tokscale only.
- A6: **an omp profile for the run**; the bench moving the entry aside; or he moves it by hand.
- omp's package: **installed at 19.1.0**, or left with the cases skipped.

## Recommendation
The bold options.

## Choice
His answers of 2026-11-23, the recommended option in each:
- **On his PC:** `h6-memory` and `h8-parallel` in a sandbox Jocasta, as H2–H5; `h1-guard live-check` beside it, since it runs none of its picks (its ACP client records each call, and a canary stops it if anything ran). H6 and H8 measure memory beside his other programs, and the thesis says so.
- **Not rerun:** `h9-footprint` and `h1-guard git-guard-check` keep their VM refusal; the thesis reports their VM results as the TypeScript build's and says why there is no Rust number.
- **Installed for the benchmarks only, pinned, outside his PATH:** opencode 1.18.22 (`npm install --prefix D:/thesis/tools/opencode opencode-ai@1.18.22`) and tokscale 4.21.0 (`npm install --prefix D:/thesis/tools/tokscale tokscale@4.21.0`, whose `@tokscale/cli-win32-x64-msvc` package carries the Windows binary; the GitHub release has no files). A bench session puts both `node_modules/.bin` folders on its own PATH. Installed 2026-11-23; both answer their version.
- **A6 through an omp profile:** the main run passes `--profile bench`. Before that run its folder takes his default folder's config, skills and extensions, and his `mcp.json` without the `jocasta` entry; he signs in there once (`omp --profile bench`, both accounts).
- **omp's package:** `bun install -g @oh-my-pi/pi-coding-agent@19.1.0` (0624: it follows the omp he runs), so H1's oracle and the 6 skipped cases run again.

## Reason
His answers in the review form.

## Consequences
- Amends 0652: the scripts it left without a machine have one, or are reported from the VM.
- Amends 0533: the main run keeps its seven arms; H9 and git-guard-check have no Rust-build row.
- Amends 0537: A7 runs on his PC; the bench no longer refuses it off the VM. Its key still reaches opencode's process alone, with the same $150 limit, and must be set before the main run.
- `bench/README.md`'s table, the A7 notes and `tokscale-parity`'s missing-tool message name the PC.
