# 0690 omp 19.2.0 is the pinned omp

- Status: v1 (built 2026-11-26)
- Kind: product, benchmark
- Phase/slice: lightweight program
- Date: 2026-11-26

## Question
He runs omp 19.2.0, a minor release past the 19.1.1 that 0663 pinned, and said on 2026-11-26 "i want that done". Does the pin follow, and does any seam of plan 5.4 and 5.5 break?

## Facts
Verified 2026-11-26 against the published 19.1.1 and 19.2.0 packages (`npm pack` of pi-coding-agent, pi-agent-core, pi-ai, pi-wire, pi-catalog and pi-utils into `target/omp-src/<version>`, `diff -r` of `src/`: 738, 5, 50, 3, 31 and 12 files differ), the installed omp 19.2.0's own `omp config list --json` under a scratch home, and every omp line Jocasta's comments cite, found again in 19.2.0 by its text.
- Step A on 19.2.0: 647 settings (644 on 19.1.1); 3 added (`tui.vimShowCommand`, `tui.vimYankToClipboard`, `tui.vimStartMode`), none removed, no default changed (`paste.largeMenuThreshold` changed only its description). All 50 key paths of `config/omp/config.yml`, `config/boss.yml`, `config/guard.yml`, `config/heavy.yml`, `config/headless.yml`, `config/review.yml`, the eval variants' `settings:` and `src/frontier/server/overlay.rs` are registered.
- The bridge extension (`config/bridge-extension.ts`): every `ctx` and `pi` member and event it uses exists in 19.2.0's `extensibility/extensions/types.ts` unchanged; that file's diff moves UI types only. New: the session event `agent_pause` (the loop stops inside one run); omp's lifecycle mirror hands extensions an `agent_end` with `willContinue: true` for it, so an extension still sees one `agent_end` per loop. The bridge judges a pane idle by `ctx.isIdle()` and its job count, not by `agent_end`, so nothing changes for it.
- The command line: `cli/flag-tables.ts` differs only in imports (46 flags in each); every flag Jocasta passes exists. New in `cli/args.ts`: `--no-tools` with `-e`, `--hook` or `--trusted-extension` is refused. Jocasta's one `--no-tools` call (`pr_actions.rs`'s reword) starts a bare omp with no `-e`, so it is untouched.
- Unchanged where Jocasta reads them: `session/session-paths.ts`, `session/compaction-methods.ts`, `modes/print-mode.ts` (`PRINT_MODE_COST_CAP_EXIT_CODE = 3`), `daemon/protocol.ts` (`__omp_worker_daemon`), `tools/approval.ts` and `exec/settings.ts` (imports only), `tools/resolve.ts`'s `proposeDispatchDetails`, the ask tool's legacy `questions` reading, pi-ai's usage and cost types. Additive only: session entries (`fallback?`, a fork-boundary custom type), `omp usage --json` (`orgPolicyRejectedAccounts?`), pi-wire (recent-tool line counts, a shell-trace runner, `waiting`, `checkout` and `toggle` events). The catalog still has `anthropic/claude-haiku-5-5` (H1's live check model); the new Anthropic names are variants of existing models.
- Changed, with an effect: the bash tool's pattern for "an argument that runs text as code" (`BASH_APPROVAL_REINTERPRETED_ARGUMENT_RE`, `tools/bash.ts:122`) went from `-[^-]*[ce]` to `-[^- \t]*[ce]`: one flag word, no longer a run across spaces. Jocasta's guard reimplementation doesn't use it, but omp's own verdict on a line with shell control can change, so H1's oracle numbers on 19.2.0 are not 19.1.1's. omp's daemon now starts in its own runtime folder (`daemon/client.ts:180-198`, `<omp state>/run/daemon`) instead of the folder of the omp that spawned it, so on Windows it no longer holds a worktree a removal wants gone; Jocasta still spares it when ending a pane's leftovers.
- Line citations moved by the new code, the cited code the same in each: `tools/bash.ts` 0 to +21, `extensions/wrapper.ts` −1, `session-tools.ts` +10, `interactive-mode.ts` −688 to −1638 (its UI moved out to controllers), `agent-session.ts` −7 to +32, `main.ts` +80 and +177, `ask.ts` −7, `resolve.ts` +1, `args.ts` +1, `bash-executor.ts` −1, `session-entries.ts` +3 to +20, `event-controller.ts` −1, `model-resolver.ts` +59, `extensions/types.ts` +25, `session-maintenance.ts` +31, pi-ai `types.ts` +1. pi-tui's `html-form/data.ts` moved into the coding agent as `tools/html-form/data.ts` (`fieldQuestions` 136–155). Each moved range was then checked against the symbol its comment names: all match except two that were already off before this bump, now corrected: the `{html}` schema is `tools/ask.ts:69-71` `askSchema`, and the blocked-status question is `extension-ui-controller.ts:1042`.
- H1's oracle: Bun's global omp package is gone from this PC (`~/.bun/install/global` holds no `package.json`), so `bench/h1-guard/omp-verdicts.ts` can't load omp from it, and its tests skip with their reason.

## Options
- **Pin 19.2.0 and re-check the seams.** Pros: the omp he runs; nothing Jocasta relies on breaks, so the cost is comments, constants and this record. Cons: H1's oracle needs a 19.2.0 install it can load.
- **Stay on 19.1.1.** Pros: none left. Cons: he no longer runs it, and `PINNED_OMP` would make H1's oracle refuse his install.

## Recommendation
Pin 19.2.0.

## Choice
Built by his word "i want that done" and his standing rule (the latest omp for development, 0623, 0651): the pin is 19.2.0 everywhere current. `PINNED_OMP` (the oracle, its Rust half and `test/config/guard.test.ts`), the footprint stub's version, the README, the manual, `bench/README.md`, the config files' "Keys checked against omp 19.2.0's source" lines and every code comment that cites a current omp shape name 19.2.0, with the line numbers moved as above; the proctree comment says the daemon no longer holds a worktree. H1's runs on the build's omp go in `D:/thesis/data/bench/h1/omp-19.2.0/`. What happened on 19.1.1 stays as history.

## Reason
A paid run must not mix omp versions, and his omp is 19.2.0.

## Consequences
- Amends 0663: the pin is 19.2.0.
- H1's corpus runs again on 19.2.0 in the benchmark round: the narrower code-argument pattern may change omp's verdict on some lines with shell control.
- H1's oracle needs Bun's global omp package at 19.2.0 (an install, his call, 0624) or `--omp <package dir>`; until then its tests skip and say why.
- The task model study's results are not rerun; 19.2.0 changed no price or session shape.
