# State of the art: Jocasta tuning (2026-11-14)

Research for the tuning program: make Jocasta lighter and cheaper in tokens, and prefer deterministic code to model steps, while the code it produces stays clean. Decided from it, 2026-11-14 and 11-15: 0599 (tune before the VM weekend), 0600 (omp 18.20.0), 0601 (completion probe off in panes), 0602 (job widgets in the boss pane only), 0603 (Graphiti out, past sessions searched in SQLite), 0604 (the tool is `history_search`), 0605 (code index removed), 0606 (Mnemopi off in Jocasta's panes), 0607 (whiteboard's tools for the boss only), 0608 (small calls), 0609 (the code-quality standard), 0610 (quality held by machine checks), 0611 (a design section in the review), 0612 (Ponytail's two lines), 0613 (the push gate), 0614 (quality and security tools), 0615 (whiteboard v0.5.2 with animations), 0616 (four more small calls), 0617 (the project-pane check through omp's turn-end event), 0618 (settings decided by trials, the builds waiting), 0619 (the omp settings the trials decided), 0620 (the advisor stays, with two more arms in the follow-up study), 0621 (a finished thread's close and commit, and the weekly production numbers), 0622 (the index's "Amended by" column, the design debts after the benchmarks), 0623 (omp 18.20.2 everywhere), 0624 (omp's install outside the package-age rule), 0625 (everything Jocasta owns rewritten in Rust, before the benchmarks). 0599–0624 are built; the before and after is at the end of section 12. What is built and measured is in `docs/status.md` and `docs/testing.md`.

How to read it:
- Every number was measured on his PC on 2026-11-14 or read from a cited source that day. `[INFERENCE]` marks what was not observed directly.
- Throwaway scripts, raw counts and the full slice reports are outside the repo, in `D:/jocasta-spikes/sota/` (reports in `reports/`). This file is the digest; the reports hold the `file:line` evidence.
- Costs are omp's API-equivalent prices, not what a subscription bills.
- Adoption tests for every proposal: **T1** fewer fixed tokens per session; **T2** a model step replaced by deterministic code; **T3** code or a dependency deleted; **T4** a measured flow gain.
- His requirement of 2026-11-14: on top of the savings, the environment still produces good, above-average, clean code. The canon he named for it: Ousterhout's "Modular Design" notes, Clean Code, Refactoring, Design Patterns and Designing Data-Intensive Applications. A fifth test follows: **Q** the proposal does not lower code quality, or names the measure that would show it.

## Summary: where tokens, money and his attention go

| # | Lever | Measured | Proposal |
|---|---|---|---|
| 1 | Agents in this repo are told to read `docs/plan.md` first (`AGENTS.md:3`) | 385 KB, about 81–96k tokens (estimate) before the first tool call | D1 (section 11) |
| 2 | Subagents | $437 of $977 own spend (45%), 98 sessions, fixed median 11.7k tokens; $330 of it ran on Fable 5.2 at $0.19 a turn, against $0.034 a turn on Sonnet 5.5 | OS-2 (section 3) |
| 3 | Advisor | $233 (24%), $216 of it in project panes; 84% of its turns on Fable 5.2 | S2, OS-3 (section 3) |
| 4 | Completion probe | $8.23 of $8.64 side spend, 22.4M prompt tokens | decided: off (0601) |
| 5 | Graphiti | $0.42 an episode, 2.9 GB of images, about 1,250 lines; one query since the cutover; no better than plain search on the same text | decided: removed, SQLite search (0603) |
| 6 | Whiteboard MCP entry | in 83% of November's sessions, called in 3 sessions ever | W1 (section 5) |
| 7 | Mnemopi recall at session start | 3 distinct items injected in this repo's sessions, all old prompts of his, none a durable fact | decided: off in Jocasta's panes (0606) |
| 8 | Decision stream | 601 records in 20 active days; in a sample of 35 comparable records the choice followed the recommendation in 32 | D2 (section 11) |

## 1. Baseline measured from omp's own session files

Source: `baseline/baseline.ts`, window 2026-11-10 to 2026-11-14 by entry time. It reads each file through Jocasta's `readOmpSession` and `withoutCopies` (0538), so entries that a moved, forked or subagent-copied file repeats count once. Two runs are byte-identical. Numbers come from omp's `usage`, `model_usage` and `contextSnapshot` entries.

Role labels are heuristic: boss = cwd ends `jocasta/data/home`; thread = cwd contains `.wt/`; subagent = session nested one folder deeper; advisor = nested file whose name contains "advisor".

| role | sessions | fixed tokens, median [min–max] | tool schemas | first prompt | peak prompt, median | own turns | own spend | side spend |
|---|---|---|---|---|---|---|---|---|
| boss | 1 | 18,379 | 8,351 | 19,822 | 61,462 | 42 | $2.44 | $0.01 |
| project pane | 10 | 10,754 [10,334–26,037] | 6,477 [2,782–8,119] | 12,178 [11,697–349,754] | 196,282 | 4,039 | $286.58 | $0.33 |
| thread | 4 | 21,669 [21,442–22,276] | 7,908 [7,908–8,147] | 26,316 [25,863–35,276] | 68,220 | 195 | $18.08 | $0.06 |
| subagent | 98 | 11,657 [4,480–29,291] | 5,887 [1,130–6,421] | 14,092 [6,727–38,715] | 119,323 | 4,121 | $437.30 | $8.23 |
| advisor (heuristic) | 15 | n/a | n/a | n/a | n/a | 3,321 | $232.94 | $0.00 |

Total own spend $977.34; side spend $8.64. Compactions in the window, copies removed and `handoff` excluded: 4.

Nested sessions by owner and model (`baseline/nested.ts`, same window and reader; the agent type, model role and model come from each file's `session_init` entry):

| nested sessions | files | own turns | spend | per turn |
|---|---|---|---|---|
| advisor, under project panes | 10 | 3,096 | $216.14 | $0.070 |
| advisor, under threads | 4 | 182 | $15.69 | $0.086 |
| advisor, under the boss | 1 | 43 | $1.12 | $0.026 |
| subagents, under project panes | 81 | 3,759 | $417.94 | $0.111 |
| subagents, under threads | 17 | 362 | $19.36 | $0.054 |

| subagent requests by model | files using it | own turns | spend | per turn |
|---|---|---|---|---|
| Fable 5.2 | 51 | 1,723 | $329.83 | $0.191 |
| Sonnet 5.5 | 29 | 1,632 | $55.65 | $0.034 |
| Opus 5.5 | 7 | 512 | $41.65 | $0.081 |
| others (Sonnet 5, Opus 5, Haiku) | 15 | 254 | $10.17 | $0.040 |

- Every advisor file sits directly under a main session, and every main-session turn in the window is in a session that has one. 2,781 of the advisor's 3,321 turns ran on Fable 5.2 ($212.01).
- 96 subagent files are one level deep; 2 are two levels deep ($2.40).
- 34 `task` agents ($239.49) and 3 `scout` agents ($20.56) started on Fable 5.2 at high with no model role, where 0591 puts `task` on Sonnet 5.5 and `scout` runs on the `smol` role. 39 of those files belong to one main session of 2026-11-13. The cause (section 12): that session's main agent wrote `task.agentModelOverrides` for the session only, after he asked for Fable; nothing was saved, so no config needs changing.

## 2. Side spend

| purpose | calls | spend | prompt tokens |
|---|---|---|---|
| task-completion-probe | 187 | $8.23 | 22,409,524 |
| job-widget | 249 | $0.15 | 1,469,033 |
| cache-warm | 3 | $0.10 | 483,363 |
| memory | 18 | $0.09 | 13,797 |
| title | 5 | $0.05 | 116,708 |
| job-caption | 205 | $0.02 | 148,707 |

- The probe's gate is `task.completionProbe` (default true): a cached side request per running subagent at +2, +5, +10 and +30 minutes, then hourly. Its only output is a percent and a label in omp's `wait` and `task` views (omp `src/task/completion-probe.ts`).
- Widgets: `async.widgets` (default true), one `widget`-role call per background job and at most one caption.
- Titles: `title.generator: fork` (default) is a side turn on the session's own model; `tiny` uses the `title` role only.

## 3. omp: version, settings and surface

Facts (reports `sota-omp-settings.md`, `sota-omp-surface.md`):
- 18.20.0 has 615 settings keys against 604 in 18.18.6: 14 added, 3 removed, no default changed. All 55 key paths in Jocasta's shipped files are still registered and valid. No seam of plan 5.4/5.5 broke in source; the collab wire gains one additive field.
- 18.20.0 trims the always-on prompt by about 3.9 KB (system prompt −3,020 bytes, `read.md` −917 bytes).
- omp#18722 is fixed since 18.18.8. Run on the installed 18.20.0: `git --version` exits 0 with `bash.gitGuard` on (127 on 18.18.6).
- Fixed cost per session counted with `omp toks` and RPC `get_state` (no model call): 10.1k tokens at omp's defaults, 11.4k in a Jocasta pane. Dividing characters by four undercounts this text by about 1.5 times. Counted sizes: `ponytail.md` 1,612 tokens, the `eval` tool 1,246, the `task` tool 1,145.
- `advisor.enabled: true` (his setting) also runs in `omp -p`; Jocasta's `headless.yml` does not turn it off.
- `shellMinimizer`, `read.summarize` and `tools.intentTracing` are on in every benchmark arm and were never isolated. Earlier evidence on output compressors points at a cost rise (`harness-os.md:19-20`).
- New in 18.20.0: `browser.webkit` (default true) downloads a WebKit engine from `build.stencil.so` on the first browser tab.
- omp 18.20.0 has no `<important if>` handling; conditional rules come from rule frontmatter and TTSR, testable offline with `omp ttsr test`.
- An extension can see `session_stop`, `plan_review_requested`, `plan_review_resolved` and `mode_changed`, and answer a plan review with `resolvePlanReview(id, choice, {role})`. Jocasta infers plan state by parsing session files (`sessions.ts`) and steers by typed text (`review-flow.ts`), because 0386 rules extensions out.

Proposals:

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| S1 | With the 18.20.0 bump, `bash.gitGuard: true` again in `config/guard.yml` | T4 | do with the bump (in 0600) |
| S2 | `advisor.enabled: false` in `headless.yml` and `boss.yml` | T2 | do |
| S3 | `title.generator: tiny` in the pane overlay (probe and widgets are 0601, 0602) | T2 | do |
| S4 | Benchmark arms isolating `shellMinimizer`, `read.summarize`, `tools.intentTracing` | T1 if measured | arms only |
| S6 | `defaultThinkingLevel: auto` as an arm | T2 if measured | arm only, after S4 |
| S7 | `--max-cost` on the unattended weekly review | T4 | do |
| OS-2 | Subagents: `task.maxRecursionDepth: 1` (−1,145 tokens each) and a lean `task` agent file (about −3.4k in all) | T1 | depth 1 now; the agent file after an arm |
| OS-3 | Advisor cadence: `reviewInterval: 3`, `agent-end`, or a cheaper role | spend | his call; it is a safety net |
| OS-4 | `eval` off in thread panes (−1,246 tokens; closes the guard's `eval` gap) | T1 | do, measured on an arm |
| OS-5 | Move three to five regex-detectable ponytail rules to TTSR rule files | T1, T2 | pilot with `omp ttsr test` |
| OS-6 | A small extension forwarding plan and stop events to the service (reopens 0386) | T3 | read-only forwarder first, if he reopens 0386 |

Not for Jocasta: `systemPromptTemplates` (forks omp's prompt), `snapcompact.systemPrompt` (loses caching), `task.steps`, `task.autoParallel`.

## 4. Memory: Mnemopi, Graphiti, the code index and plain files

### What ran (reports `sota-memory.md`, and `memory-value/` for this section's own runs)

Graphiti with FalkorDB, started for this test with `docker compose up --wait` and stopped after it:
- It starts healthy in 23 s. Memory in use: Graphiti 473 MiB idle, 702 MiB after 30 searches; FalkorDB 28–40 MiB. Images 2.9 GB.
- The "not working since 11-12" in the cost report is a restart artefact, not a broken graph: the Graphiti container exits with 143 when stopped, its restart policy is `on-failure`, so Docker Desktop restarts it at boot without FalkorDB (which exited 0). The demo project's container shows `restarts=2` the same way. [INFERENCE from exit codes, restart counts and start times.]
- The graph has no full-text index. `db.indexes()` lists range indexes only; every search logs `Received 5 arguments to procedure 'db.idx.fulltext.createNodeIndex', expected at most 1` (66 errors over 31 searches). Graphiti's hybrid search has therefore run on embeddings alone since it shipped.
- Content: 14 episodes (12 handoff summaries from 3 sessions, 2 decision notes; 159k characters), 122 entities, 162 facts. 71 facts (44%) were already ended, within the one day of ingestion: the episodes are working state, and six handoffs of one session arrived within 30 minutes.
- Cost: $5.82 for the 14 episodes ($0.42 each, 98.9% on the large model), recorded as `cost: 0` in `graph-usage.jsonl`, so "$ today" misses it.

Detail survival: of the identifiers in the episodes, how many can still be found anywhere in the graph (facts, entity names and summaries)?

| identifier | distinct in episodes | found in graph |
|---|---|---|
| decision record ids | 86 | 27 (31.4%) |
| file paths | 183 | 47 (25.7%) |
| commit hashes | 28 | 6 (21.4%) |
| backticked names | 847 | 224 (26.4%) |
| version numbers | 17 | 12 (70.6%) |

The comparison plan 5.0 asked for, in a reduced form with no model call: 30 questions (the `## Question` text of every third decision record the episodes cite), three arms.

| arm | corpus | answer contains the record id (of 30) | answers the question, judged on 10 | mean characters returned | time per query |
|---|---|---|---|---|---|
| Graphiti `/search`, 10 facts | the 14 episodes, as a graph | 8 | 3 | 972 | 108 ms |
| SQLite FTS5 (BM25), 10 lines | the same 14 episodes, as lines | 14 | 3–4 | 1,355 | 1 ms |
| SQLite FTS5 over the decision index titles | `docs/decisions/README.md` | right record first: 12; in the first five: 22; in ten: 24 | — | — | under 1 ms |

38% of the facts the graph returned were ended ones. The judged column is the author's reading of the first five answers per question against the record's Choice; it is a small sample.

Mnemopi since the cutover:
- At session start it injected 7 blocks into this repo's sessions; 4 were non-empty, holding 7 items, 3 distinct. All three are verbatim earlier prompts of his (a pasted test-failure list fixed the same day, a review request carried out in 0573, an install request carried out in 0595). None is a durable fact. In other folders: 11 blocks, 3 non-empty.
- The recall tool: no call in this repo's sessions. In other folders 9 calls: 4 answered "no relevant memories", 1 "not initialised", 4 returned transcript fragments; one question was re-asked in other words straight after its answer. In the first 420 characters of those four answers no item addresses the query.
- Cost: about 330 fixed tokens a session and $0.02 a day. The boss's bank never got a row: no recall, no extraction, no file.

Plain files, same window, this repo's sessions (2,979 tool calls): 248 calls touched `docs/decisions/`, 126 `docs/plan.md`, 97 `docs/status.md`, 79 `docs/testing.md` or `CHANGELOG.md`. That is 550 calls, 18% of all, against 0 for recall and 0 for `graph_search`.

The code index (codebase-memory-mcp) was never built under Jocasta: no index folder, no call; 318 MB of binary and 604 lines of manager.

### What the literature says (report `sota-memory-lit.md`, 21 sources)

- Every published benchmark of graph memory is chat memory. Zep's margin over full context on DMR is 0.2–0.4 points; LoCoMo has three vendor-run scores for one product (58.44, 75.14, 94.7). No coding evaluation of Graphiti or Zep was found.
- LongMemEval-V2 (451 questions over agent trajectories, full text read): a coding agent over files scored 69.9–74.9% against 38–59% for RAG-style memory, at 100–190 s per query, with a small reader model.
- A controlled study gave Claude Code with Sonnet 4.6 a code-graph MCP tool over 300 tasks: no Pass@1 gain, cost up 8–28% in four of eight settings. It is another tool of the same class as the code index.
- SWE-ContextBench (99 tasks, one run): an oracle-selected 217-token summary lifted resolve rate from 26% to 34%; the same summaries chosen by the agent gave 22%; full trajectories cost 27% more for no gain; off-the-shelf memory frameworks scored 24–30%.
- Developer-written guidance beat generated guidance (cost −7.9% to −41.7%, about twice the agent-synthesised kind); generated context files added about 20% cost with no gain in success.
- Limits: mostly single runs, no Anthropic frontier model on repository work with a memory service; at Jocasta's volume a success-rate A/B cannot be measured, only deterministic measures can.

### Proposals

V9 asks for "persistent memory, a knowledge graph or SQLite", so memory stays in some form; the question is which.

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| M1 | Remove Graphiti and FalkorDB: service, image, LLM endpoint, watcher's post path, container manager's resource. The search tool answers from SQLite FTS5 over the summaries `catalog.db` already keeps | T2, T3 | decided (0603); the tool becomes `history_search` (0604) |
| M2 | Remove the code index and its mise pin; relationship questions go to omp's `lsp` | T3 | decided (0605) |
| M3 | Mnemopi off for the boss (`memory.backend: "off"`), and in panes either off or recall by tool only (`autoRecall: false`) | T1, T2 | decided: off for the boss and the panes (0606) |
| M4 | If Graphiti stays instead: fix the restart policy, the missing full-text index and the zero cost, and move extraction to the small model | — | not taken |

## 5. Whiteboard

Facts (report `sota-whiteboard.md`):
- Pinned v0.2.4; upstream is v0.5.2, eight stable releases in five weeks. 0.5.x adds animations (experimental, off by default), which V6 names. The spike of 0608 then checked the seams (report `whiteboard-052-spike.md`): the server side is compatible, the canvas bridge is not (Home throws, a review answers 404), the tutorial is gone upstream, and the animation stage needs a serving exception; a port of about 180 lines renders, animations included. Decided and built as 0615.
- The `whiteboard` MCP entry is in the user-level `mcp.json`, so every pane, thread and subagent mounts it and starts a Node process. Its only consumer is the boss.
- Prompt cost per pane: about 137 tokens when the server is not up at pane start (537 of 543 sampled sessions), about 1,300 when it is (estimates from omp's route template).
- Use: 41 tool calls in 3 session files, none after 2026-10-14; the boss's `show` twice. Mounted in 83% of November's sessions.
- Behind it: about 2,940 lines of canvas host, Monaco, and a 195 MB runtime.

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| W1 | Move the MCP entry from the user-level file to the boss's own file, with a migration | T1 | do |
| W2 | Replace the MCP entry with a boss skill plus `whiteboard api` (about 50 tokens fixed) | T1, T3 | later, if W1 is not enough |
| W4 | Bump to v0.5.2 for animations (V6) | none | only with a spike; no token gain. Spiked, then taken (0615) |
| W6 | Drop diagrams | T3 | no: V6 is a vision item |

## 6. herdr

Facts (report `sota-herdr.md`):
- 0.9.5 is still the latest stable; no release for 31 days. Master has about 20 fix commits after it, among them a server crash (#5303) and a slow-server socket (#5511), which Jocasta's read-retry and revival code works around. [INFERENCE: no master build was run.]
- Measured on an isolated scratch server: 16 MB working set with no panes, 27.6 MB with 16 idle shells; CPU 0.16% of a core idle, 3–4% at 8–16 shells. An omp thread pane is about 640 MB.
- Jocasta uses 27 of 102 socket methods, 1 of 3 event kinds, 0 of 26 hook events.
- A fork would prune about 14% of the source, need Rust 1.96.1 and Zig 0.16, and save nothing measurable at runtime.
- herdr-core (`hc` 0.1.21) has an equivalent for 11 of Jocasta's 27 methods.
- `herdr config check` passes on the shipped `herdr.toml`; nothing in Jocasta runs it.

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| H1 | Keep 0.9.5; bump when a release carries #5303, #5511 or #5510 | none | keep |
| H2 | No fork | none | keep |
| H3 | Run `herdr config check` in setup and in a gated test | T2 | do |
| H5 | Report the HUD through sidebar tokens instead of a command polled every 5 s | T3 | low priority |
| H6 | Port to herdr-core | — | not now; reopen if no TUI release by 2027-01-14 |

## 7. Windows

Facts (report `sota-windows-env.md`; three checks needed elevation and were skipped):
- omp's bash tool is an embedded engine: PowerShell, cmd and Git Bash start-up cost is not on the agent path. `omp --version` takes about 1.06 s against 10–54 ms for Bun alone; cause not isolated.
- A `pre-push` hook injected through `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_0`, `GIT_CONFIG_VALUE_0` in panes rejected plain, `git.exe :main` and alias pushes to main in a scratch test; `--no-verify` bypasses it.
- Redirecting `gh`'s config folder does not hide his login: with an empty `GH_CONFIG_DIR`, `gh auth token` still printed a token. Pane env is not scrubbed.
- Microsoft Execution Containers (announced 2026-10-07) can sandbox processes without admin setup and this build is above their floor. Whether a TUI pane can run under them is open.
- No Dev Drive. Bun installs took 3.8–6.6 s with the cache on another volume and 2.7–3.2 s on the same volume (2–3 runs per cell).
- `core.longpaths` is unset and `LongPathsEnabled` is 0.

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| WE1 | Env-injected `pre-push` hook for panes, plus a guard rule for `--no-verify` | T2 | do |
| WE3 | `core.longpaths` in the same injected config | T4 | do with WE1 |
| WE4 | Bun's cache on the projects' volume | T4 | do |
| WE5 | A time-boxed spike of Microsoft Execution Containers for thread panes (needs an SDK install) | — | his call |
| WE2, WE7 | git fsmonitor; Dev Drive | — | not now |

## 8. Packages

Facts (report `sota-omp-packages.md`):
- The catalogue lists 8,532 npm packages; 130 marked "Works", which means the package loaded, not that a tool was called; 92% untested.
- Installing a package runs `bun install` and then its code inside omp's process with his credentials. The installer has no sandbox.
- Ponytail's plugin rule text is identical to the text vendored in `config/ponytail.md`.
- A tool-deferral package would hide the thread's own `check` and `report_result`; omp's `xd://` catalog already defers schemas.

Recommendation: adopt none. One optional library-only spike: cc-safety-net against the guard's own command list (`config/guard.yml:12-15`), PowerShell forms unverified.

## 9. Stencil blog, omp², prewalk and snapcompact

Facts (report `sota-stencil-omp2.md`; no omp session run):
- omp² shows no sign of release: the only npm tag is `latest: 18.20.0`; branch `omp2` last moved on 2026-09-04; issue #9820 is closed with "no public release date or ETA".
- Prewalk (blog, SWE-Bench Pro, sample size and intervals unstated, never an Anthropic small model as executor): Opus 4.8 alone 85% at $2.78; prewalk 78% at $1.46.
- `f-prewalk` (A6 plus `--prewalk`) is confounded: A6 starts in plan mode, and omp turns `prewalk.auto` off there because the plan-file write "would trip a handoff". [INFERENCE from `prewalk.ts`; not run.]
- Plan review in 18.20.0 offers "Approve and keep context" and a role slider. Only "Approve and execute" drops the research context.
- The blog's snapcompact numbers use a 6×10 font; the shape that ships for Anthropic models is about 4.4 characters per billed token, roughly 1.1 times denser than text. [INFERENCE from arithmetic.]
- Haiku 5.5 is capped at a 100,000-token window in omp unless `extendedContext` is on.

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| P1 | Replace `f-prewalk` with a clean arm without plan mode, with a detector for the switch | T4 | do before the rerun |
| P2 | Add `f-keep-smol` beside `f-keep-context` | T4 | do |
| P3 | A four-arm compaction test with a follow-up recall question; keep 0592's order until it reports | T4 | arm only |
| P4 | Pilot `snapcompact.toolResults` and read its savings journal | T1 if measured | arm only |
| P5 | `mise run omp2-watch`: npm tags and the branch tip | T2 | do |
| P6 | Accept `"true"` and a bare string for a list in `src/mcp/server.ts` | T2 | only if the ledger shows such rejections |

## 10. Literature: rules, compaction, subagents, verification

All eleven papers were read in full and every sentence below checked against the text (report `sota-paper-validation.md`). Transfer to Jocasta's setting (long sessions, frontier Anthropic models, omp) is medium for five and low for six; none is high.

Rules and always-on text
- A policy written as a condition is followed less than a direct instruction. Claude Code with Opus 5: 78.0% prescriptive, 77.2% explanatory, 70.7% prohibitive, 64.2% conditional; with Sonnet 5: 71.6, 49.4, 17.3, 35.6. A reason made no significant difference on Opus 5; on Sonnet 5 the explanatory wording was significantly worse. One marker-comment policy, one wording per style, 86 tasks, three runs (arXiv 2611.04367; transfer medium).
- Context files did not generally improve success (p=0.87, 0.37, 0.21) and raised cost by 20–23% for generated files and up to 19% for developer-written ones; instructions in them are followed; repository overviews did not help the agent reach the relevant file sooner (arXiv 2602.11988 v3; medium).
- A stale instruction was followed whenever the task led through what it named: the test command in 78 of 80 runs, the procedure in 55 of 80, the code location in none of 84. Removing the line prevented most stale-path use; correcting it prevented all of it; task success did not change (arXiv 2611.09802; medium).

Memory
- Memory raised resolve rate from 35.3% to at most 42.6% on tasks the agent failed without it and caused failures on tasks it solved alone (harm 4.8–9.9%). Raw trajectories cut steps by about 34% but used 2.5–5.1 times the input tokens (arXiv 2610.27524; DeepSeek in Claude Code; low).
- Rebuilding guidance from raw trajectories on demand and discarding it gave 59.9% held-out success against 49.0% for persisted abstractions, with the lowest call cost; not a coding benchmark (arXiv 2610.27406; low).
- A designed memory beat full context only with the weakest model; one conversation, one run per cell (arXiv 2610.29823; low).

Compaction
- With two 1M-context models in OpenCode, thresholds from 160K to 380K tokens did not measurably affect success. In 44 real sessions, process instructions and scope exclusions were dropped 35.9% and 36.8% of the time against 6.2% for feature requirements (arXiv 2610.31778; medium).
- Across 14 summarizers, 5% to 64% of 22 repairs that succeeded from the full history failed from a summary (Opus 5.5: 45%, wide intervals, histories of at most ten steps). Summaries kept at most 16.0% of recurring literal details, a mechanical extractor 79.1%, but retention did not predict task success (arXiv 2610.31710; low).

Subagents, roles and preamble
- A planner-coder-reviewer pipeline on Opus 5.5 cost 3.22 times one call and solved no more tasks. With Haiku 5.5 as planner and reviewer it cost 0.39 times the all-Opus pipeline, lost 6.7 points, and still cost 1.27 times one Opus call. An oracle picking the better model per task would reach 94.3%; a real zero-shot router reached 71.4%. 135 short tasks (arXiv 2610.37700; low).
- Preamble per step: Claude Code 16,581, OpenCode 7,025, mini-SWE-agent 829 tokens; cost per task differed up to three times with the same model. omp was not measured (arXiv 2610.04433; medium for the preamble).

Verification
- An independent critic improved accuracy 5.9–9.5 points on a 31-task Terminal-Bench set at 1.13–1.25 times the cost, over seven non-Anthropic models with no significance tests. Completion gates gained at most about as much at 1.5–1.8 times the cost (arXiv 2610.34577; low).

HumanLayer joined Factory on 2026-11-04; it is not a living upstream. Its stop hook (silent on pass, full output on failure) is what Jocasta's idle check already does.

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| R1 | Keep always-on rules imperative; no conditional rewrite | none | keep |
| R2 | A deterministic lint that every command and path named in the always-on files still resolves | T2 | do, as a test |
| R4 | After a compaction, re-assert the "must not" list from files, by code | T2 | after the task file ships |
| R6 | No role-split subagent pipeline | none | keep |
| R7 | Keep the one independent critic and the silent idle check; no completion gate | none | keep |

## 11. Design audit: Ousterhout's modular design and the No AI Slop manifesto

Sources read in full: the CS 190 "Modular Design" notes and slopmanifesto.com. Metrics are regex-based (report `sota-design-audit.md`, raw counts in `design-audit/raw/`).

Facts:
- `src`: 146 files, 31,103 lines; none over 2,000 lines; tests are 0.92 of source lines; repeated code is near zero (0 identical eight-line windows across files).
- Deep modules: `herdr.ts` (one interface used from 33 files), `proctree.ts`, `fsx.ts`.
- `service.ts`: 17 of its 26 public methods only forward to a part (`service.ts:393-513`); 37 imports, the repo's maximum; tests call the forwards 77 times.
- `ServiceContext` is a 19-field bag every part receives, built with a cast and filled in an order that matters (`service.ts:209-227`).
- omp session cost is interpreted in three places with different rules: `sessions.ts`, `ledger.ts`, `llm-endpoint.ts`.
- 96 of 855 exports have no other file naming them; 19 are values.
- 133 named timing constants in 57 files; the owner-facing `jocasta.yaml` has about 14 keys.
- `JOCASTA_LAYER` and `JOCASTA_HOSTS` are test-only switches that production code branches on.
- Text: `docs/plan.md` 385 KB; section 3, which restates decisions, is 35% of it and appendix I 26%. `AGENTS.md` is 15 KB with one 6,300-character line. Jocasta's own prompts are small: `ponytail.md` and the boss manual.
- Decisions: 601 records, 191k words, on 20 active days (30 a day, 159 on 2026-10-08). 240 of them come after 0363.
- Recommendation against choice, read from a sample of every fifteenth record: 35 comparable, 32 followed the recommendation, 3 did not.
- The only automatic checks are typecheck, lint and tests. Rules that already hold by hand and a script could check: index rows equal record files, every `JOCASTA_*` variable is in appendix I, Bun's version is equal in three files.

Reading, by the two sources:
- Ousterhout: the service facade is the "method that simply calls another" red flag; the context bag is information leakage; the three cost readers are one job in three places.
- Manifesto: belief 5 (review cost stays constant while production cost falls) and belief 8 (the next agent reads and copies what sits in the knowledge base) apply to the plan mandate and the decision stream; resolution 12 (let a check catch what a check can) applies to the hand-held process rules.

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| D1 | `AGENTS.md` stops telling agents to read the whole plan: it names the sections to read and sends them to the records by index | T1 | do; then generate plan section 3 from the records |
| D2 | Fewer questions: unchanged, low-stakes recommendations go on one tick-list per slice by default (0089), and 0363's builder-chosen class widens to what is local and reversible | T4 | his call |
| D3 | A `docs-check` test: index and records in step, required record lines, env vars in appendix I, version pins equal, cited records exist | T2 | do |
| D4 | Remove `export` from the 19 unused value exports; a warn-only task for new ones | T3 | do |
| D5 | Delete the six `key*` forwards by giving the plugin its `Keys`; then one method that owns the stop order | T3 | do when that code is next touched |
| D6 | One reader of omp usage and cost in `omp-session.ts`, used by all three | T3 | do, with a fixture test that both pipelines agree |
| D7 | Merge the one-consumer route files (`health-route`, `door-routes`, `feed`) into `routes.ts` | T3 | do |
| D8 | Remove the two test-only switches after the benchmark program closes | T3 | later |

## 12. Code quality: the canon, what corrects code today, and the gaps

His requirement of 2026-11-14 turned the question around: not only what can be cut, but what keeps the code good once it is. Five reports (`sota-quality-canon.md`, `sota-quality-mechanisms.md`, `sota-quality-sessions.md`, `sota-quality-lit.md`, `sota-quality-tools.md`) and two spikes (`hook-spike.md`, `skillspector-spike.md`).

How much of the canon was read that day: Ousterhout's CS 190 notes (modular design, complexity, comments, exceptions, testing) and the method-length, comments and TDD parts of his debate with Martin, in full. For Clean Code, Refactoring, Design Patterns and Designing Data-Intensive Applications only the tables of contents and catalog pages: their chapters are cited from knowledge and were not re-read.

### The canon as a rubric

- 60 rules drawn from the five sources. 47 can be checked by a program: 13 have a Biome rule, 4 a compiler option, 40 a script or a test (some have two). 13 need a reader.
- Biome has no rule for commented-out code, duplicated code, comment quality, flag parameters or missing interface comments.

Where the sources disagree, and what fits a 31k-line TypeScript codebase that agents write:

| Point | Martin | Ousterhout | Fit, and the measurement behind it |
|---|---|---|---|
| Function length | very small | depth matters more; split only when the parts are clean | Ousterhout, with ceilings for the outliers: the median named function is 7 lines and the 90th percentile 28, so "smaller" has no room left and would breed shallow helpers |
| Comments | a comment is a failure to express in code | interface comments are essential | Ousterhout: 96.8% of exported functions carry a comment, and a hand sample of 66 comments found none that restates its code |
| Test-first | tests first, in short cycles | tests with the change; always one for a bug | per project: much of Jocasta is glue to herdr, omp and the OS, where a test cannot lead the design |
| Class size | small, one reason to change | 200 to 2,000 lines is fine behind a small interface | Ousterhout for lines, Martin for the interface: `Day` fails on 26 public methods, not on 936 lines |
| Asserts per test | one concept, ideally one assert | several small tests | one behaviour per test; 222 tests with more than 5 expects are a prompt to read them |
| Names | a long name beats a comment | long names are awkward | Ousterhout: a short name, and the comment carries the contract |

### The code as it stood (census of `src`, 142 files, 30,730 lines)

| Measure | Value |
|---|---|
| Named functions: median, 90th, 99th percentile, longest | 7, 28, 93, 199 lines (`ThreadStarts.start`) |
| Functions over 100 lines | 13 |
| Cognitive complexity over 15 / 25 / 40 (Biome) | 92 / 36 / 11 |
| Functions with more than 5 parameters | 2 |
| Files over 500 / 1,000 lines | 12 / 3 |
| Boolean parameters a branch tests (flag parameters) | 21 |
| Functions that only forward | 64, 21 of them on `JocastaService` |
| Exported items with a comment: functions, public methods, types, classes | 96.8%, 86.3%, 67.9%, 62.7% |
| Commented-out code | 0 |
| Optional share of 1,158 record properties; `readonly` share | 33.1%; 3% |
| Empty `catch`: all, with a stated reason | 32, 29 |
| `as` casts / `any` / non-null `!` | 289 / 0 / 0 |
| Tests: files, tests, mocks, skips | 93, 804, 0, 1 |
| Duplicated lines (jscpd 5.4.2): `src`, `test` | 0.18%, 1.98% |
| Unused exports and exported types (knip 6.44.2) | 69 and 28 |
| Import cycles | 2 (one real, in the canvas) |

Reading: the code has no long-function problem, it has a handful of real ones. Comment practice is strong and contradicted the shipped Ponytail line "comment only the why". The weak spots are wide records with many optional fields, the forwarding facade, casts, and copies written in different styles: the review of 0573 found 25 duplicates that a text-window detector cannot see.

### What corrected code in his sessions (2026-11-10 to 11-14)

| Step | Cost in the window | What it visibly corrected | Per correction |
|---|---|---|---|
| Jocasta's `check` tool (threads only) | no tokens; 4 runs of about 100 s | 2 failing runs, 1 fail, fix and pass chain | $0 |
| Diagnostics shown at an edit | not separable | 13 re-edits within 10 minutes of 18 error results | not separable |
| Check commands agents ran themselves | at least $48.89 | 6 fail, fix and pass chains in 669 runs | about $8 |
| The advisor | $232.94, 173 notes | about 30% of a 60-note sample acted on: all 6 blockers, none of 15 nits | about $4.50 per acted note |
| Reviewer runs | $66.36, 13 runs | 6 runs followed by an edit to a file the review named | about $11 |
| Plan mode | $15.26, 2 sessions | none: both plans approved unchanged | none seen |
| The strongest model on `task` subagents | $239.49, about $188 over the usual price | none attributable: equal tool-error rate, different work | none seen |

- None of the 4 project-pane sessions that edited ran a check after its last edit. Project panes and their subagents are where $704 of the window's $977 went.
- Could a program have caught the 242 findings of the full review (0573)? A static check 58, a test 29, a reviewer model 144, only he 11.
- Subagents do not see Ponytail or `AGENTS.md`: a quality rule in always-on text covers the main session only.

### What the literature says (report `sota-quality-lit.md`)

Read levels: 7 sources in full, about 28 by abstract, method, results and limits, 5 reused from the earlier paper slice. Numbers below are as the sources report them.

- **Passing tests is a weak proxy for mergeable code.** Maintainers merged test-passing agent patches about 24 points less often than the grader passed them, and for newer Claude models the rejections moved toward code quality (METR, read in full).
- **Volume predicts structural decay; prompts did not reduce it.** Lines of code against architectural smells: Spearman 0.94 over 20 generated systems; few-shot design guidance did not cut long methods (no Anthropic model in the study).
- **Claude models that cheat on a conflicting task do it mostly by editing the tests** (over 79%; ImpossibleBench). Read-only tests stop the edits; model monitors missed 35 to 58% of cheating.
- **Agent-written tests are weak safety nets:** more value printing than assertions, more mocks than humans write, under 60% mutation score.
- **The strongest model reviewing its own draft changed little** (91.4% left unchanged); a different strong model reviewing a weaker draft lifted it from 71.6% to 89.7% (116 single-file tasks).
- **A reviewer asked for design problems tends to add layers.** Anthropic's own guidance warns that fixing every finding yields extra abstraction and tests for cases that cannot happen. No study measures it. Hence 0611's cap of three findings that do not block and that name code to delete.
- **Lint-style feedback changes what agents refactor, at 3 to 5 times the tokens** (small open model; the metric is the one shown to the agent).
- **Rules text has a weak measured effect and a measured cost;** vendors now recommend moving must-hold rules into hooks. Process rules were dropped about 36% of the time after compaction, against 6% for feature requirements.
- **No study was found** that measures over-engineering, dead code, defensive fallbacks, comment noise or scope creep as rates in agent code, or schema migrations, retries and transaction boundaries.
- **Made-up package names:** 3.6 to 5.0% of package mentions on 2026 models, at least 5.2% for commercial models in an older study (abstracts).
- **Two numbers need their scope beside them.** The 38.9% of agent pull requests with a security smell (arXiv 2607.12428) covers only pull requests that touch CI, Docker, infrastructure, config and shell files, with the smells labelled by a model. Anthropic's "$15 to $25 a review" and "under 1% of findings marked wrong" are vendor claims about its Code Review product, not about `/security-review`.

### Gaps, and what was decided

| Gap | Decided | Record |
|---|---|---|
| The books disagree and no shipped text says which reading holds | The five sources are the standard; Ousterhout's reading where they disagree; one rule set for new work and maintenance | 0609 |
| Biome ran only its recommended set; nothing measured size or complexity | Eleven more rules, three compiler options, ceilings of 40, 120, 5 and 1,000 | 0610 |
| A diff could delete tests or assertions unseen | A removed test, fewer assertions or an added skip fails the review | 0610 |
| Jocasta's own repo had no task the check on idle looks for | A `verify` task | 0610 |
| The check on idle ran in threads only | Project panes too, through omp's turn-end event | 0610, 0617 |
| No review step looked at design | A design section: the rubric's judgement rules, at most three findings that do not block, naming code to delete; the same rubric for the weekly drift agent | 0611 |
| Two Ponytail lines contradicted the canon | Rewritten; one removed | 0612 |
| Flag parameters, forwarding methods and undocumented exports are invisible to Biome | Syntax-tree checks that fail on new cases only | 0610 |
| Dead code and duplication | knip in the check; jscpd's share in the scorecard | 0614 |
| Dependencies: 5 advisories in `dompurify`; made-up or hijacked packages | An override; `bun audit` in CI; packages must be 7 days old | 0614 |
| A push to trunk needed no approval | Asked once per repo, through a git hook from the pane's environment | 0613 |
| Drift is an impression, not a number | A `scorecard` mise task the weekly review runs and compares week on week | 0610, 0616 |

Not taken, with the reason: mutation testing (StrykerJS 10.1.1 crashes at start with TypeScript 7); a security pass in every review (no cost or precision figure); Trivy in Jocasta's own repo (`bun audit` covers its dependencies); `eval` off in thread panes (a thread's subagents used it in 101 of 638 calls); a wider class of builder-chosen details (101 of 613 records differ from their recommendation, and the rule stays).

### Two spikes

- **omp's turn-end event** (omp 18.20.0, about $0.30 on the cheapest model). Text sent to a pane is pasted at the cursor and the whole box is submitted: a pane holding the draft "draft" submitted "draftReply with exactly: PONG2". An extension's `session_stop` handler that answers `{continue: true, additionalContext}` gives the model another turn with that text as a hidden message; a draft typed during the turn stayed in the box unchanged. It is called for the main session only, again after every settle, and is cut off after 30 seconds. Token counts were identical with and without the extension. Decided as 0617.
- **SkillSpector** v2.13.4, static mode, over everything Jocasta ships: 5 findings, all false positives, two rated high (a licence text, the word `.env`). It was not tried on a planted bad prompt. Not a release check (0614).

### Measures for the before and after

Deterministic, at no token cost: typecheck and lint diagnostics; the scorecard's counts under the ceilings; jscpd's share; knip's findings; the review proof's removed tests and assertions; gate failures and blocking notes per thread. Judged, and not run in this program: a rubric score by a model of another family, the reviewer's recall on seeded defects, and his own rating of a blind sample of 20 diffs, which is the one gold standard. The benchmark's primary metric, hidden-test pass, cannot see code quality.

### Before and after the builds (2026-11-15)

Deterministic only, at no token cost. "Before" is the scorecard of 2026-11-15 ahead of the knip, lint and tree-check builds, and `fixed.ts` on omp 18.20.0; "after" is the tree with every build of 0599–0624, on omp 18.20.2.

| Measure | Before | After |
|---|---|---|
| Functions over cognitive complexity 25 (`src`) | 36 | 31 |
| Functions over 80 lines | 13 | 10 |
| Functions over 4 parameters | 22 | 20 |
| Files over 800 lines | 5 | 2 |
| jscpd, `src` | 0.18% (5 clones, 52 of 29,605 lines) | 0.17% (5 clones, 52 of 29,856 lines) |
| jscpd, `test` | 1.96% (65 clones) | 1.93% (66 clones) |
| knip findings | 98 | 0 |
| `bun audit` | 5 advisories (`dompurify`) | none |
| Typecheck and lint, under 0610's rules and options | not enforced | 0 errors; offenders beyond the ceilings fixed or marked with a reason |
| Tree checks (flag parameters, forwarding, undocumented exports) | not counted | 22, 33, 137 held as the baseline; a new case fails |
| Fixed tokens, a project pane, the omp bump alone (both sides already after 0606 and 0612) | 10,874 (omp 18.20.0) | 11,022 (omp 18.20.2) |
| Fixed tokens a pane's subagent loses without the `task` tool (0619; measured on a pane's prompt at depth 0) | | −1,495 |

The pane's 148 more tokens are omp 18.20.1's selective-commit text, which 0623 keeps on (−151 when off); `task.maxRecursionDepth: 1` and the turn-end extension of 0617 change no token of the pane itself. The program's own savings on a pane's fixed tokens (Mnemopi off, 0606; Ponytail's lines, 0612) were built before tier 0a's first run and are not in this table. The session-based numbers of section 1 (spend, side requests, peak prompts) are not measured again: the installed Jocasta is the build of 2026-11-10, so sessions since then still run the old overlay. They can be compared once he installs this build and works on it for some days, on the same reader and window length.

## 13. Settings decided by trials (2026-11-15)

His word on 2026-11-15: the settings are tried, and the choice follows what the trials show. Three tiers, cheapest first; scripts and raw results are in `settings-trials/`.

- **Tier 0a, fixed tokens, no model call** (`fixed.ts`): omp in RPC mode with a scratch agent folder and a dummy key answers one `get_state`; its system prompt and tool list are counted with `omp toks` under the Opus 5 tokenizer. The pane is a project pane's command line as `pane-launch.ts` builds it, after the 0606 and 0612 builds.
- **Tier 0b, his own sessions, no model call** (`tools-census.ts`, `settings-census.ts`, `advisor-notes.ts`, `cache-retention.ts`): omp's session files from 2026-11-10 to 11-15, 208 files, $1,264.79 at omp's prices; counts only, no content.
- **Tier 1, real trials** (`screen.ts`, `analyze.ts`): the repo's own `runTrial` on the 12 cases of `evals/cases/`, six arms, two reps, 144 trials, one at a time on Opus 5.5 at high. `base` is a Jocasta pane's config (arm A1's) with the advisor pinned off; each other arm changes one setting. The arm that goes first rotates from case to case, because a trial reads the prompt cache the trial before it wrote. A trial is priced from omp's own `usage.cost` in its session files: the session, its advisor and its side requests. Intervals are a bootstrap over cases, 10,000 resamples. One trial was cut off when the session running the screen restarted, and was run again.

All 144 trials passed. These cases decide cost and time at equal pass; they cannot show a difference in quality.

| Fixed tokens (tier 0a) | System prompt | Tools | Total | Against a pane |
|---|---|---|---|---|
| omp's defaults | 3,257 | 6,756 | 10,013 | |
| a Jocasta project pane | 4,116 | 6,758 | 10,874 | 0 |
| pane, `tools.intentTracing: false` | 4,075 | 6,758 | 10,833 | −41 |
| pane without the `task` tool (what a subagent at its depth limit gets) | 3,865 | 5,514 | 9,379 | −1,495 |
| pane without Ponytail | 3,177 | 6,758 | 9,935 | −939 |

`advisor.enabled`, `read.summarize.enabled`, `shellMinimizer.enabled`, `defaultThinkingLevel` and `snapcompact.toolResults` change no fixed token.

| Arm (tier 1, 24 trials each) | Passed | Spend | Against base [95%] | Wall time against base [95%] | Detail |
|---|---|---|---|---|---|
| `base`: advisor off, thinking high, 1-hour cache | 24 | $2.15 | | | 118 turns |
| `providers.cacheRetention: short` | 24 | $1.66 | −23% [−28, −17] | −3% [−9, +1] | cache writes $1.21 → $0.75, the same tokens |
| advisor on, every update | 24 | $3.94 | +83% [+59, +110] | +21% [+13, +30] | 114 advisor requests, $1.84 |
| advisor on, every third update | 24 | $3.69 | +71% [+48, +102] | +21% [+11, +34] | 56 requests, $1.53 |
| advisor on, at agent end | 24 | $3.47 | +61% [+43, +82] | +29% [+21, +41] | 32 requests, $1.34 |
| `--thinking auto` | 24 | $1.63 | −24% [−33, −10] | −8% [−15, −1] | low in 12 trials, medium in 12; 100 turns; its classifier cost $0.001 in all |

The same case run twice on `base` differed by 3% at the median and 14% on average (76% at most).

What the trials show:
- **The advisor in `omp -p` runs.** It wrote 8 notes in 72 trials: 3 concerns and 5 nits. Two concerns named a defect the cases' checks cannot see (a parameter type that rejects the module's own row type under `tsc`). All 8 arrived with no assistant message after them: the run had ended. With `advisor.syncBacklog: strict`, in 6 more trials, 1 of 2 notes was followed by a turn. In print mode the advisor is spend and time that does not reach the result.
- **The advisor's cadence.** Against every update, every third update lowers the advisor's spend by 17% [−27, −8] with 51% fewer requests; at agent end by 27% [−33, −22]. The spend falls less than the requests because $1.26 of its $1.84 is cache writes: a skipped update is still sent with the next review.
- **The advisor in his sessions** (five days):

| Watched session | Sessions | Advisor spend | Against the watched sessions' own spend | Cache writes in the advisor's spend | Blockers, concerns, nits | Spend a note | Notes that came after the session's last turn |
|---|---|---|---|---|---|---|---|
| project pane | 15 | $285.20 | 66% | 49% | 12, 121, 28 | $1.77 | 0 of 161 |
| thread | 4 | $15.69 | 87% | 79% | 5, 11, 2 | $0.87 | 0 of 18 |
| boss | 1 | $1.12 | 46% | 60% | 0, 0, 1 | $1.12 | 1 of 1 |
| weekly review (`omp -p`) | 1 | $0.26 | 84% | 86% | none | | |

  Replayed on those sessions, every third update is an estimated 28% to 34% less for the advisor under project panes and about 14% under threads (cache writes kept; cache reads and output scaled by the screen's ratios, or by a third).
- **Cache retention.** With no pause between requests, `short` (5-minute entries at 1.25 times the input price, against 1-hour entries at 2 times) is 23% cheaper at omp's prices. Replayed on his sessions, with a miss after every pause over 5 minutes that crosses a message of his, and one refresh per 4.5 minutes inside a run: −6% in all; subagents −17%, threads −19%, project panes −6%; the advisors of project panes +17% and the boss +87%. The token counts never fall, and every miss writes tokens again (14.9M in the replay). omp's `auto` already writes 5-minute entries on an API key and 1-hour entries on a subscription login.
- **Three settings that are always on, in his sessions.** `tools.intentTracing`: 79% of 19,153 tool calls carry the label, 11.9 output tokens each, 1.4% of all output tokens (about $4 of $285). `shellMinimizer`: rewrote 101 of 7,153 bash results, 850 KB down to 514 KB; the raw output was fetched back 13 times (56 KB). `read.summarize`: 222 of 4,776 reads were answered with a summary that left out 42,055 lines (about 1.9 MB); 133 of them (60%) were followed within eight calls by a re-read of the same file (149 re-reads, 981 KB).
- **Subagent depth.** `task` was called once in 137 subagent sessions; the tool and its prompt text are 1,495 fixed tokens in each. Never called in any of 176 sessions: `lsp`, `ast_grep`, `ast_edit`, `debug`, `github`, `security_scan` (behind omp's tool devices they cost a route line, not a schema).

| Setting | Decided by | Outcome |
|---|---|---|
| `advisor.enabled` in `config/headless.yml` (the weekly review) and in the benchmark's base arm | print-mode notes arrive after the run; +61% to +83% spend | off there (S2 for headless) |
| `advisor.enabled` in `boss.yml` | $1.12 in five days, one session, one nit | no change: nothing to measure a gain on |
| `advisor.reviewInterval: 3` (0616) | −17% of the advisor's spend at equal pass; 28% to 34% estimated under his project panes | stays |
| `task.maxRecursionDepth: 1` (OS-2) | one nested call in 137 subagent sessions; −1,495 fixed tokens in each | do, in the panes' overlay |
| `providers.cacheRetention` | cheaper only at API prices, and dearer where he pauses; omp's `auto` already follows the kind of login | stays `auto` |
| `shellMinimizer`, `read.summarize`, `tools.intentTracing` (S4) | the label is 1.4% of output tokens; the minimizer rewrote 1.4% of bash results; summaries answered 4.6% of reads, leaving out about 1.9 MB of which 1.0 MB was read back in 149 more calls, so their net is not measured and its sign is unclear | stay on, as omp ships them; no benchmark arms: about 3% of read bytes against 149 calls is below what the follow-up study can show |
| `defaultThinkingLevel: auto` (S6) | −24% spend at equal pass on easy cases; it never chose `high` there | not adopted on easy cases alone: an arm of the follow-up study, his call |
| the advisor in panes and threads | 17 blockers and $301 in five days; no benchmark arm runs it | his call: section 14 |
| `snapcompact.toolResults`, the compaction order (P3, P4) | need sessions long enough to compact; the 12 cases never do | remain arms |

**omp 18.20.2 under the trials (0623).** At 00:50Z on 2026-11-15 omp's updater replaced bun's `omp.exe` shim with 18.20.2's standalone binary; bun's global package, whose source H1's guard tests load, stayed 18.20.0. So 131 of the 144 trials ran on 18.20.0 and 13 on 18.20.2 (each case and rep on one version), and the 6 `syncBacklog` trials on 18.20.2. Read from the two sources (`omp-18.20.2/`): 184 of 1,962 files differ; 20 of the 60 omp files the plan and Jocasta's code cite changed.

| On 18.20.2 | Measured or read | For Jocasta |
|---|---|---|
| fixed tokens | defaults 10,161 (+148), a pane 11,022 (+148); the settings' differences above are equal | 0619 stands |
| `commit.enabled`, new and on | −151 in a pane when off: the whole of the +148 | stays on (0623): in a project pane it keeps his own edits out of an agent's commit |
| `todo.midRunNudges: capped`, new | 0 fixed tokens; a reminder after 12 edits or commands, at most twice | omp's default |
| grep context 1 and 3 lines → 0 and 0 | 0 fixed tokens; smaller results, not yet measured | omp's default; the before-and-after names the version |
| `bash.patterns` merge across layers, the overlay's first | the first match still decides; his own config has none | the guard decides first; H1's resolver tests run on 18.20.2's source with the bump |
| `advisor/settings.ts`, `task/types.ts`, the ACP permission gate | identical; `task/executor.ts` and `task/index.ts` changed, their depth check (`atMaxDepth`, `canSpawnAtDepth`) did not; `session_stop` is as it was | 0619, 0620 and 0617 rest on the same logic |

## 14. The factory view: three texts, the playbook and Ousterhout, against Jocasta

His word on 2026-11-15: more philosophy behind the decisions before building. He gave three texts (an AI software factory, read as the industrial revolution's lesson; ontologies as knowledge an agent can compute over; an enterprise path to an autonomous SDLC) and two links read again that day: the Harness Playbook and Ousterhout's "Modular Design" notes.

One reading joins them: what lasts is the system around the model. A factory's advantage is quality control, reusable parts, the economics of each unit and a loop in which every job improves the next. The SDLC text ends on it: "Autonomy is less about choosing the smartest model and more about engineering the system around it." The ontology text asks for knowledge the agent can check itself against, and its author calls that a hypothesis still to be measured.

| Lesson | Jocasta as it stands | Evidence | Gap |
|---|---|---|---|
| Quality control at the line (factory; SDLC step 1: strict gates a looping agent can validate against) | the project's check cached by git tree, `done_when`, the review with its proof, the push gate (0613), the pane check (0617), the rubric (0609, 0611), lint ceilings and tree checks (0610) | 0610–0617; `test/docs/` | none new; three decided builds wait |
| Reusable components (factory; SDLC step 2: one versioned configuration every repo inherits) | `config/` ships with the release; one overlay per project; `jocasta setup --refresh`; skills, agents, the boss manual | 0593, 0607; 55 key paths valid on omp 18.20.0 | none |
| The economics of a unit (factory: cost, latency, throughput) | the ledger prices every request; `today` and `thread_status` show a thread's spend and waste | `ledger.ts`; `thread.json` holds `createdAt`, `spend`, every review | no time of finish; no table of finished threads: lead time, spend a finished thread, share passing the first review (F1) |
| The compounding loop (factory; SDLC steps 3 and 4: learn from session histories; trace a merged commit back to its session) | lessons on his word (0459), read at plan time and by reviewers; `history_search` over past sessions (0603); the weekly review with the scorecard (0583, 0616) | 2 lessons and 2 thread folders in the live install | the squash commit names the branch, not the thread: no path from a commit to the thread and its session (F2) |
| Knowledge to compute over (ontology text) | the records with their `amends` links; plan appendix I as the ledger of seams; lessons with `applies_when` and `retire_when`; `test/docs/` checks index, appendix I and version pins | 616 records, 450 amend links to 227 records | 107 of those 227 records, and their index rows, do not name every record that amended them: a reader of the index can act on a changed decision (F3) |
| Retrieval is not that knowledge | graph search removed (0603), the code index removed (0605) | the graph named the right record in 8 of 30 questions, text search in 14; the code index was never built on his PC; `lsp` was never called in 176 sessions | none: the measurements support both removals |
| Remote, isolated, event-triggered runs with approval (SDLC step 5); the whole SDLC (step 6) | his Allow gate; worktrees; one machine | 0304, 0485 | after v1 by those records |
| Playbook: one authoritative session, a trusted control plane, bounded work, explicit compatibility, views as projections | recorded as 0515, to enter the plan with the omp² move | 0515 | none now |
| Playbook: every schema has a tax; limits are part of the primitive | tools behind devices; `task` depth | section 13 | closed by section 13's depth limit |
| Ousterhout: pass-through methods, one job in several places, thin files | 0610's tree checks hold today's count as the baseline | the two readers of omp's cost agree to the cent on 207 session files ($1,662.15 each) | D5, D6 and D7 of section 11 are still open; D6 is duplication, not a wrong number (F4) |

| ID | Proposal | Test | Recommendation |
|---|---|---|---|
| F1 | `thread.json` gains the time a thread closed; the weekly review gets a deterministic step beside the scorecard: threads finished that week, lead time, spend a finished thread, share whose first review passed | T2, T4 | do: the loop gets numbers at no token cost |
| F2 | `thread.json` gains the squash commit's id, so a commit leads to its thread, task file and session; nothing is written into the project's history | T2 | do; a commit trailer is the alternative, and it shows the tooling in every client repository |
| F3 | The index's rows carry "amended by", generated from the records' own `amends` lines and checked in `test/docs/` | T2 | do |
| F4 | D5, D6 and D7 as one refactoring slice after the benchmarks, measured by the scorecard | T3, Q | after the benchmarks: a refactor before them changes what they measure |
| F5 | Two arms in the follow-up study: the advisor (on A6, whose work turn is interactive) and `--thinking auto` (on A1), 20 trials each | T4 | his call: it is the only way to weigh the advisor's $301 in five days against the defects it stops |

## 15. Rust rewrite: facts

Facts (report `sota-rust-facts.md`; the floor numbers come from throwaway Rust and Bun programs built with the toolchains already on the PC):
- `jocasta.exe` is 118.9 MB and a Bun-compiled one-line script is 115.3 MB: Jocasta's own code and assets are about 3.6 MB, 97% is the Bun runtime. The install is 429 MB: `jocasta.exe` 119, hunk 117, whiteboard 105, Node 24 90.
- Idle floor: a Rust process blocked on stdin uses 4.3 MB working set and 0.7 MB private; a Bun-compiled one 17.0 MB and 106.3 MB.
- Start: `jocasta --help` 35 ms and the `mcp` handshake 33 ms, against 26 ms for a Bun hello, 18 ms for a Rust hello and 22 ms for `herdr --version` (noise about ±5 ms). `jocasta --version` takes 744 ms, 626 ms of it waiting for `omp --version`.
- One active omp tree: about 2.8 GB working set, 4.8 GB private, 574 CPU-seconds in 51 minutes. Its `jocasta mcp`: 21.6 MB and 1.1 CPU-seconds in 61 minutes. A sandbox Jocasta stack (service, `today`, launchers, four MCP servers) summed to about 490 MB working set in one sample.
- Ties to TypeScript: no omp extension and no import of an omp package. What stays JavaScript in any case: the browser canvas page (1,963 lines) and whiteboard's Node host. The benchmark's oracle loads omp's TypeScript (`bench/h1-guard/omp-verdicts.ts:139`); 94 of 113 test files import `src/` internals; 38 of 201 bench and eval files import product code.
- herdr runs any argv command as a plugin and has no daemon slot: the long-lived service is outside its plugin model in any language.
- Crates exist for each dependency; two are unverified against Jocasta's use: YAML output byte-compatible with omp's (`serde_yaml` is deprecated) and iCalendar recurrences (`node-ical`).
- Toolchain present: rustc and cargo 1.93.1, MSVC 14.44; nothing to install.
- Size of a port: about 19,200 lines for the service, the panes and the MCP server; 25,900 with `shared/`; 28,700 with setup. The 28,216 lines of tests do not carry over as they are.
- Precedent: GitHub's Copilot runtime port (vendor claim, read in full) moved 430k lines of TypeScript to 830k of Rust in 14.5 weeks; start went from 5.25 s to 292 ms and ten-client memory from 1,383 MB to 126–247 MB, with dozens of regressions; its goal was an embeddable runtime. No published before-and-after for a tool of Jocasta's size was found.

| ID | Option | Effect | Tests met |
|---|---|---|---|
| RS1 | No rewrite | none; every test and benchmark stays valid | none |
| RS2 | Rust only for the per-pane `jocasta mcp` | about 15 MB working set less per pane and 15 ms per spawn, 0.5% of an omp tree [INFERENCE from the floor] | none |
| RS3 | Rust for the service, the panes and the plugin roles; canvas and whiteboard stay JavaScript | Jocasta's own working set from about 490 MB to a few tens [INFERENCE]; the exe from 119 MB to a fraction; 19–26k lines ported, tests rewritten | T3 in part |
| RS4 | Rust for the herdr plugin roles only | 17 ms per keypress; about 1,600 lines; one more binary | none fully |

No option changes tokens (T1) or replaces a model step (T2). omp, not Jocasta, dominates memory and CPU in the daily flow. A rewrite before the benchmarks changes the artifact under test.

Decided as 0625 (2026-11-15): more than RS3. Everything Jocasta owns is ported to Rust, before the benchmarks; the canvas host page stays TypeScript, and whiteboard, hunk, herdr and omp are external. The recommendation was RS1.

## 16. Not verified

- Code quality: the four books' chapters were not re-read; syntax-tree counts used the TypeScript 5.9.3 parser, not the repo's 7.0.2; the advisor's acted-on share rests on 60 notes, 57% of them undecidable; no judged measure was run; the turn-end event was not tried with a real check or a real edit turn inside the handler.
- The advisor's rows carry no context snapshot, and whether its notes are re-sent each turn was not checked.
- One project pane has a 349,754-token first prompt; unexplained.
- Rendered fixed-prompt tokens after the 18.20.0 bump (only file bytes were compared).
- Graphiti: search quality with a working full-text index; the comparison used the graph as it ships. The judged column in section 4 is one reader's judgement on ten questions.
- Mnemopi: whether a human would call any recalled item useful beyond the items seen; the recall answers were read through their first 420 characters.
- Whiteboard: real tokenizer counts; on v0.5.2, playback beyond the first frame, the tour, clip blocks, Home's actions and several pages at once in Jocasta's own page.
- herdr: no master build was run. Windows: Defender exclusions, Dev Drive trust and `powercfg /requests` needed elevation.
- Design audit: no AST; plan section 3 was not diffed against the records.
- Papers: figures and most appendices were not read; no replication package was opened.
- Settings trials: the 12 local cases are easy (every arm passed every trial), so nothing here speaks for quality; how a subscription's windows count 1-hour cache writes; the cache replay models warming and misses by rule and observed neither; `shellMinimizer` and `read.summarize` were counted in his sessions, not switched off in a trial; `--thinking auto` on tasks where `high` matters; whether the advisor's notes land in time in A6's interactive turn.
- Factory view: the three texts were read as given, their sources were not traced; F1 to F3 are designs, none was built or tried.
