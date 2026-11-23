# 0663 omp 19.1.1 is the pinned omp

- Status: v1 (built 2026-11-23)
- Kind: product, benchmark
- Phase/slice: fork build, phase 8
- Date: 2026-11-23

## Question
He runs omp 19.1.1, a patch past the 19.1.0 that 0653 pinned, and said on 2026-11-23 "yes move the version". Does the pin follow, and does any seam of 0653's list break?

## Facts
Verified 2026-11-23 against the published 19.1.0 and 19.1.1 packages (`target/omp-src/19.1.0` and `19.1.1`, six packages each, `diff -r` of `src/`) and the installed omp's own `omp config list --json` under a scratch home.
- Step A on 19.1.1: 644 settings (643 on 19.1.0); 2 added (`todo.backend`, default `native`; `computer.takeover`, default `fallback`), 1 removed (`computer.backgroundFallback`, which became `computer.takeover`: omp's settings migration carries `refuse` over), no default changed. All 52 key paths of `config/omp/config.yml`, `config/boss.yml`, `config/guard.yml`, `config/heavy.yml`, `config/headless.yml`, the eval variants' `settings:` and `src/frontier/server/overlay.rs` (with the relay overlay) are registered with their old defaults; none of the three changed keys is used.
- Nothing Jocasta relies on broke. Unchanged line for line: `cli/args.ts` (`--continue`, `--prewalk`), `modes/print-mode.ts` (cost-cap exit code 3), `cli/usage-cli.ts`, `collab/registry.ts` and `relay-client.ts`, pi-wire (the whole package), `tools/approval.ts`, `tools/resolve.ts` (`propose`), `exec/settings.ts` and `exec/bash-executor.ts` (git guard), `session/session-entries.ts`, `session/session-paths.ts`, `session/compaction-methods.ts`, `mcp/protocol.ts`'s constants, `mcp/host/legacy-session.ts`, `daemon/client.ts` and the daemon's worker argument, the extension events and the handler limit (30 s, `EXTENSION_HANDLER_TIMEOUT_MS`).
- Changed, with no effect on Jocasta: the MCP era probe now also reads `data.supported` of an unsupported-version error (a server listing a handshake-era revision is taken as one; a bare `-32022` is still "modern"), and a modern link re-probes after such an error; `jocasta mcp` answers the probe with method-not-found, which still falls back to `initialize`. The ask tool refuses a fieldset whose radios each have their own `name` (Jocasta authors no such forms; the question text it extracts is unchanged). Plan mode's `mode_change` to `plan` carries `prePlanModelIdentity` and `prePlanThinkingLevel` beside `planFilePath` (additive). A `model_change` entry and the restored model name a routed model as `provider/id@upstream` (Jocasta reads neither). An idle daemon (no client, no unit holding it) exits after 3 s, which only shortens the time Jocasta's spared-daemon logic matters. New `todo.backend` extension point, `computer.takeover`, `restoreTerminatedBashJobs` on `--restore`, a tree position in the status line: none read.
- Line citations moved by the new code: `tools/bash.ts` +13 (`BashTool.approval` 834–894), `extensions/wrapper.ts` +14, `session-tools.ts` +10, `interactive-mode.ts` +6 to +51, `agent-session.ts` +11 to +63, `main.ts` +25, `ask.ts` +1 and +23, `model-resolver.ts` +94, `exit-diagnostics.ts` +27, `session-maintenance.ts` +2; the cited code in each is the same.
- H1's oracle: his global Bun package is now 19.1.1 with its sources, so `bench/h1-guard/omp-verdicts.ts` finds and loads it (its `load` answer names 19.1.1), and `jocasta-bench h1-guard omp-verdicts --any-version` ran over the 1,285-line corpus (the binary built before this record still held 0653's `PINNED_OMP`): 499 lines decided by a guard pattern, 30 by the git guard, 756 by the mode default; 32 lines differ from the reimplementation, all allow to deny. The approval code is the same as 19.1.0's, so the run is a first measurement on 19.1.1, not a comparison with a 19.1.0 run (none exists).

## Options
- **Pin 19.1.1 and re-check the seams.** Pros: the omp he runs; his rule for development is the latest; nothing breaks, so the cost is comments, constants and paperwork. Cons: none found.
- **Stay on 19.1.0.** Pros: the task model study ran there. Cons: he no longer runs it, and `PINNED_OMP` would make H1's oracle refuse his install.

## Recommendation
Pin 19.1.1.

## Choice
Built by his word "yes move the version" and his standing rule (the latest omp for development, 0623, 0651): the pin is 19.1.1 everywhere current. `PINNED_OMP` (the oracle, its Rust half and `test/config/guard.test.ts`), the README, the manual, `bench/README.md`, the config files' "Keys checked against omp 19.1.1's source" lines and every code comment that cites a current omp shape name 19.1.1, with the line numbers moved as above. H1's runs on the build's omp go in `D:/thesis/data/bench/h1/omp-19.1.1/` (no 19.1.0 folder was ever written). What happened on 19.1.0 (0653's fixes, the task model study) stays as history.

## Reason
A paid run must not mix omp versions, and his omp is 19.1.1.

## Consequences
- Amends 0653: the pin is 19.1.1; its fixes stand.
- The task model study's 19.1.0 results are not rerun; 19.1.1 changed no code that decides a verdict, a price or a session shape.
- For his review (`fork-build.md` For him 27): H1's corpus run on 19.1.1 is a first measurement, to be repeated by the benchmark's own command once the build is rebuilt.
