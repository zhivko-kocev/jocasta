# omp: what could go upstream (draft, 2026-11-15)

His word on 2026-11-15: "at the end of the sessions we also can draft a file for stuff we can improve on omp we had 8 PRs merged". This is that file: what the tuning program measured or ran into in omp (`can1357/oh-my-pi`), written so each item can become an issue or a pull request. Which items are sent, and how, is his call.

How to read it:
- Every item was observed on his PC (Windows 11) on omp 18.20.0 or 18.20.2 and checked against omp's source at 18.20.2. On 2026-11-15 the head of omp's `main` is the 18.20.2 version bump, so "on 18.20.2" also means "on `main`".
- Numbers come from `docs/research/state-of-the-art.md`, section 13, and from the records it led to. Scripts and raw results are outside the repo, in `D:/jocasta-spikes/sota/` (`settings-trials/`, `omp-18.20.2/`).
- Costs are omp's API-equivalent prices.
- "Related" names an upstream issue found by search on 2026-11-15. Where one exists, the item is a data point for it, not a new issue.
- The builds of 0619–0623 have not run yet. What they find is added here before anything is sent.

## 1. Already upstream

Eight pull requests merged, three open, one issue closed. All eight merged before 18.20.1 was published and are in 18.20.2.

| Number | State | What |
|---|---|---|
| issue 18722 | closed 2026-11-10 | Windows: with `bash.gitGuard` on, the shell's `git` builtin could not start git (exit 127). Fixed in 18.20.0; the guard is back on (0600) |
| 19095 | merged 2026-11-13 | a seance id of another project hints at the `.jsonl` path |
| 19096 | merged 2026-11-13 | settings docs: `computer.display` defaults to active |
| 19106 | merged 2026-11-13 | a configured Mnemopi `dbPath` uses native separators on Windows |
| 19107 | merged 2026-11-13 | the shell's POSIX errno numbers and texts off Unix |
| 19108 | merged 2026-11-13 | the shell runs the `.exe` that `find -exec`, `xargs` and `env` resolve on Windows |
| 19146 | merged 2026-11-13 | the shell keeps Windows drive roots in zsh and files paths |
| 19169 | merged 2026-11-14 | zsh `mkdir -p` starts past a Windows drive root |
| 19170 | merged 2026-11-14 | unmapped Win32 errors map to `EIO` off Unix |
| 19097 | open | Mnemopi's `llmMode: smol` described as the memory role |
| 19098 | open | docs: `getHashedIndex` uses xxHash32 |
| 19109 | open | the incomplete-UTF-8-at-EOF decoder test pins UTF-8 |

## 2. Candidates

### A. Defects with a repro

**U1. A bash call with `pty: true` loses every in-process builtin.**
- Without `pty`, `type jq` answers "jq is a shell builtin" (omp's own shell, bash 5.3.0). With `pty: true` the same line runs in the system's `/usr/bin/bash` (5.3.9) and answers "not found" for `jq`, `sd`, `fd`, `rg`, `sponge` and `ast-grep`; the coreutils resolve to the system's. `jq --version` exits 127.
- The tool's description lists these as "in-process builtins, ahead of PATH" and says only that `pty` is interactive. Internal URLs as paths, which the builtins understand, are lost the same way.
- Ask: say in the tool text that a `pty` call runs the system shell without the builtins, or put them on that shell's PATH. Seen before and after the update to 18.20.2. No related issue found.

**U2. In print mode the advisor's notes arrive after the run has ended.**
- `omp -p` with `advisor.enabled: true`, 72 trials on 12 small cases: the advisor wrote 8 notes (3 concerns, 5 nits), and none was followed by an assistant message. The runs cost 61% to 83% more and took 21% to 29% longer than with the advisor off. With `advisor.syncBacklog: strict`, 6 more trials: 1 of 2 notes was followed by a turn.
- `advisor.enabled` carries `protocolDefault: ["rpc", "acp"]`, so those hosts start with it off; print mode takes the user's value.
- Ask: treat print mode like the protocol hosts, or hold the exit until the last review is delivered, as `--wait-async` does for background jobs.
- Jocasta's side is decided (0619): the weekly review and the benchmark's base arm pin the advisor off. No related issue found.

**U3. `omp update` on a bun-global install on Windows leaves two omps of different versions, silently.**
- At 2026-11-15 00:50Z `~/.bun/bin/omp.exe` (bun's 8 KB shim) was renamed to `omp.exe.<ms>.<pid>.0.bak` and the 252 MB standalone binary of 18.20.2 took its place; `omp.bunx` is not there now. Bun's global `@oh-my-pi/pi-coding-agent` and its sibling packages stayed at 18.20.0, untouched since their install two days earlier.
- `cli/update-cli.ts` takes a launcher over like this in two cases: bun ran, exited 0 and the launcher still reported the old version (`updateViaManager`, which then removes `omp.bunx`), or `omp.bunx` was already missing and the bare `.exe` was taken for omp's own binary. Either way nothing says so afterwards: `omp --version` reports 18.20.2 while anything that reads the package's `src/` reads 18.20.0.
- The update also ended the omp session that was running.
- He ran the update himself. Not known: which of the two cases it was.
- Ask: after a takeover, say which global package is now stale and how to remove it. Related: issue 8659 (a stale nested `pi-natives` after a bun-global update), issue 10518.

### B. Proposals with a measurement

**U4. The advisor's spend is mostly cache writes.**
- In five days of his sessions the advisor cost $285.20 under 15 project panes and $15.69 under 4 threads; 49% and 79% of that is cache write, at the 1-hour rate of a subscription login. In the trials, reviewing every third update (`advisor.reviewInterval: 3`) cut its requests by 51% and its spend by 17%, because a skipped update is still sent with the next review.
- Ask: a cache retention of the advisor's own, or `providers.cacheRetention` per model role. Related: issue 7226 (the advisor's full-transcript replays).

**U5. `read.summarize`: 60% of summaries are followed by a read of the same file.**
- In five days of his sessions (2026-11-10 to 11-15), 222 of 4,776 reads were answered with a summary that left out about 1.9 MB; 133 of them were followed within eight calls by another read of that file (149 reads, 1.0 MB). The net is not measured.
- Ask: skip the summary when the path was named by the user or by an earlier tool result in the turn; the thresholds (`read.summarize.minTotalLines` and its siblings) exist but not that rule. Related: issue 1473.

**U6. One 30-second limit for every extension handler.**
- `EXTENSION_HANDLER_TIMEOUT_MS` is 30,000 for all events; `extensionHandlers.toolCallTimeoutMs` covers `tool_call` only. A `session_stop` handler that runs a project's check has to give up at 27 seconds (0617).
- Related and open: issues 11286 and 15290 ask for exactly this. A data point for them, not a new issue.
- Found in the 0617 build's second review: a handler's result is discarded when the turn's settle is aborted (Esc), and `SessionStopEvent.signal` is the only way a handler learns it (`extensibility/shared-events.ts`, `extensibility/extensions/runner.ts`). Jocasta's extension now ends its work on that signal. No ask beyond a line in the extension docs.

**U7. The subagent completion probe costs more than it shows.**
- `task.completionProbe` (default on) made 187 side requests in four days: $8.23 of $8.64 of all side spend, 22.4 million prompt tokens, for a percent and a label in the `wait` and `task` views. Jocasta turns it off in its panes (0601).
- Related and open: issue 17408 (run the probe on a chosen model role). A data point for it; the further ask is "off by default".

**U8. One `--append-system-prompt`; which arrays merge is not written down.**
- The flag keeps the last value given (`cli/flag-tables.ts`), so a launcher that adds two texts has to join them into one file first, as Jocasta does for a thread.
- Since 18.20.1 `bash.patterns` and `bashInterceptor.patterns` merge across layers and across `--config` files (the higher one first); every other array still replaces the lower one. An overlay can therefore no longer drop a lower layer's bash rules, and a project's own rules now take part behind an overlay's.
- Ask: a repeatable `--append-system-prompt`; one line in the settings docs per array saying "merges" or "replaces". No related issue found.

**U9. `omp usage --json` has no written schema.**
- Jocasta's pacer reads `reports[].limits[].amount.usedFraction` and `window.resetsAt`, found by reading `cli/usage-cli.ts`.
- Ask: document the shape, or version it. Related: issues 19144 and 14128 add fields to it.

**U10. The `commit` text is in every session.**
- `commit.enabled` (new in 18.20.1, on) adds 151 fixed tokens to a session, subagents included: 96 in the system prompt and 55 in the eval tool's text (Opus 5 tokenizer; 11,022 against 10,871 in a Jocasta pane).
- Ask: load it on first use, as the other eval namespaces' docs are. Jocasta keeps it on (0623). No related issue found.

**U11. omp's per-user daemon starts in the folder of whichever omp spawned it.**
- omp 19.1.0 starts `__omp_worker_daemon` detached from the first omp that needs it, with no `cwd` for a compiled binary (`daemon/client.ts:180-191`, `subprocess/worker-client.ts:228-233`), so it inherits that omp's working folder and, on Windows, stays a child of it. Seen live: the daemon's parent was an interactive omp. It then holds a project folder (a git worktree cannot be removed while it does) and a launcher that ends a closed pane's process tree takes the machine-wide daemon with it (Jocasta spares it by its command line, choice 100).
- Ask: spawn the daemon with a neutral `cwd` (the home directory or omp's agent directory), so it never holds a project folder. Observed on 19.1.0 only; no related issue found.

### C. Data points, no ask

- **`shellMinimizer`** rewrote 101 of 7,153 bash results (850 KB down to 514 KB); the raw output was fetched back for 13 of them, 8 under the `chain` filter.
- **`tools.intentTracing`:** 79% of 19,153 tool calls carry the label; it is 1.4% of output tokens and 41 fixed tokens.
- **Tools never called in 176 sessions:** `lsp`, `ast_grep`, `ast_edit`, `debug`, `github`, `security_scan`. Behind the tool devices each costs a route line, not a schema.
- **Subagent depth:** `task` was called once in 137 subagent sessions, and the tool with its text is 1,495 fixed tokens in each. `task.maxRecursionDepth: 1` removes it there (0619). Related: issue 19077 (depth 0 removes the tool from the root session too).
- **A repeated `--thinking`:** the last one wins (`cli/flag-tables.ts`), as for most value flags; the follow-up arm `f-think-auto` relies on it to override A1's level (0620). Not written in `--help`.

## 3. Checked and not sent

- **A `deny` or `prompt` rule missed a command behind `NAME=value` words.** 18.20.1 matches the segment again without them (`tools/bash.ts`). Fixed upstream.
- **grep's context lines.** 18.20.1 made the default 0 and 0.

## 4. Before anything is sent

- Search again for a duplicate of each item; several areas move daily.
- One issue or pull request per item, with the repro or the count and no session content, project names or config values.
- U1 and U3 first: they have a repro and no related issue.
