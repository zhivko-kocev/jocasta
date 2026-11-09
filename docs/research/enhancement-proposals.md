# Enhancement proposals from the 2026-11-01 research

- Date: 2026-11-01.
- Asked for: suggestions that make Jocasta better and the thesis more valuable, drawn from his links (Stencil's harness playbook and omp², Loka's "Stop writing tools", Destiner's "Pi is the last harness", eight HumanLayer posts, Matsakis on agents and Rust, Typemill's tasks as code, Miessler on writing), from herdr's core/app split, and from skill sets to try (Karpathy, Pocock, Ponytail and others).
- Made from: six research reports, each source fetched that day and quoted with its URL. The full reports are in `D:/thesis/sources/proposals-2026-11/` (`stencil-playbook.md`, `tools-and-pi.md`, `humanlayer.md`, `thought-tasks-rust.md`, `rust-ui-herdr.md`, `skill-approaches.md`); this file cites them by name.
- Status: proposals only. One finding was a v1 bug, fixed under 0363 (below). Everything else waits for his decision: after-v1 items are grilled when their phase opens (0086), and exploratory benchmark arms run only after the pre-registered A0–A4 run, never interleaved with it (0482).

## Two v1 findings in the review's proof (0453)

1. **Fixed: a piped test command proved a pass.** omp's shell runs without pipefail (omp 18.13.1's `executeBash`: `false | cat` exits 0), so a review turn's `bun test 2>&1 | tail -20` exits as `tail` does, and `checkedTests` took that 0 as "tests pass"; `|| true`, `; …`, a background `&` or a next line did the same. A run now proves a pass only when nothing after the command can give the exit code (`ownExit` in `src/frontier/server/sessions.ts`), else the review reads "tests unverified". Source: HumanLayer's hook "Do not pipe test, typecheck, or quality output to head/tail/grep/etc." (`humanlayer.md` P1). The review prompt is unchanged, so arm A3's copy of it still matches (B7 below).
2. **Open: session state is read in file order, not along the active branch.** omp keeps a session as a tree: each entry names its `parentId`, a rewind or a `/tree` jump moves the leaf back, and later entries hang off the earlier one, so the abandoned turns stay in the file (omp 18.13.1 `session-manager.ts:556-561`, `:2080-2083`, `:3903-3958`; the replay rule `session-entries.ts:350-358`: each entry sets the leaf to its `activeLeafId`, else its own id). omp derives its state from that branch (`getBranch()`). Jocasta's `addLine` (`sessions.ts`) folds every line in file order, so a `TESTS: pass`, a plan proposal or a bash run on an abandoned branch counts until the new branch writes a newer one. The trigger is a rewind across a review or a plan approval: rare, and the review's `since` bound narrows it. The playbook names the same failure in Pi's own plan-mode example: "plan mode restored from the whole file, not the selected branch" (`stencil-playbook.md` P1).
   - Fix sketch: cost, spend and message times stay in file order (abandoned turns were billed); the tail keeps each entry's parent and a compact effect; an entry that extends the leaf applies its effect at once, an off-branch entry (one with `activeLeafId`) changes nothing, and any other leaf move replays the effects from the root to the new leaf.
   - Tests: a branched session (proposal, rewind, new turn) shows no plan waiting; a rewound review's `TESTS: pass` doesn't count.
   - Effort S–M. His call: before the cutover, or after v1.

## What the sources say

| Source | What matters here | Jocasta today |
|---|---|---|
| Stencil, "The Harness Playbook" (C. Bölük, 2026-09-02; public) | omp's postmortem and omp²'s design. One authoritative session journal; "Policy and session ownership stay on the host; sandboxes receive only bounded execution requests"; "Every permanent tool taxes every turn" (five tools: 36.6 s against Codex's 42.2 s); verification the agent can't redefine. | Views are projections (0392); a kill boundary (0486); proof before done (0453); local voice and dictation (0418, 0330). Lacks branch-aware state (finding 2), OS isolation (after v1, 0485) and a loop-owned plan gate (the 2 s settle timer). |
| omp² (verified 2026-11-01) | Not released: npm's only tag is `latest: 18.15.0`, and issue #9820 says "no public release date or ETA". The public `omp2` branch is a Rust rewrite (MIT, last public commit 2026-09-04): "expect renames and breaking changes without notice". It would break the YAML `--config` overlays (cfg scripts there) and the `.jsonl` session readers (`.oms` journals), likely `APPEND_SYSTEM.md` and herdr's TypeScript integration [INFERENCE]; it adds `--plan-mode`. | Pinned to 18.13.1 until A5 is done (0500); a bump re-checks the seams (0383). |
| Loka, "Stop writing tools" (P. Kalinovski, 2026-10-20) | Replace bespoke tools with AGENTS.md, skills and CLIs (their four requests on Opus 5.5: "116 → 25" tool calls, "≈$7.35 → $0.88"); scripts the agent writes bypass safeguards, so credentials stay behind a proxy outside the sandbox. | The boss's MCP tools are that proxy (the service's token, the boss key 0484, Allow/Deny); the bypass is 0485's accepted risk. |
| Destiner, "Pi is the last harness" (2026-11-01) | Pick an open, vendor-agnostic, minimal engine: "bloat especially harms the frontier models"; "'Plan mode' was a thing; now it's legacy. Human code review is dying." | 0386 chose the opinionated engine (whole omp, unforked); it agrees on one engine with Jocasta's code outside it. |
| HumanLayer, eight posts (2025-12 to 2026-10) | Backpressure silent on pass, full on fail, and no piping of test output; short context files, `<important if>` blocks (anecdotal: "We don't have a rigorous explanation"); long-context models degrade, so their warning moved to 100k tokens on 1M windows; forking to explore designs; their open-sourced orchestrator (Apache 2.0: desktop app, a daemon over Claude Code, Codex, OpenCode and Pi, a cloud API). | omp's shell minimizer and 50 KB spill; Ponytail's "save test output to a file" (0469); rare procedures as skills (0465); compaction at 300k (0448). |
| Matsakis (2026-10-14), Typemill (2026-10-25), Miessler (2026-10-23) | Tests aren't enough with agents, and newtypes strengthen guardrails; task, plan and report in one private file; intent written with its checks, and work checked against the intent rather than the last draft. | Records before code (0002; 314 of 507 records carry an amendment); about 142 of 549 tests cite a record; eval cases pair a prompt with checks. But a thread's brief carries no checks, and the reviewer never sees the brief. |
| herdr's split (two repos, created 2026-10-16) | `herdr-core` (`hc` 0.1.16): "Core draws nothing", JSON-RPC described in OpenRPC, "preview", with no workspace, tab, worktree, `agent.prompt` or `layout.apply` methods. `herdr-app` (0.1.11): GPUI, Windows x64; JavaScript "mods" draw panels but "can't: use HTML, CSS, a web view"; telemetry on by default. The herdr TUI (0.9.5) "keeps working", but they "don't share agents". | The herdr seam is one module (`herdr.ts`, 0392); moving to `hc` is a port, not a bump. |
| Skill sets (`skill-approaches.md`) | The "Karpathy skills" are one four-principle CLAUDE.md written by Jiayuan (forrestchang), not by Karpathy, with no LICENSE file. Pocock: 39 small skills (MIT), whose `tdd` and `code-review` stop to ask the user. Ponytail 5.3.3's AGENTS.md is byte-identical to `config/ponytail.md`. Superpowers (MIT) is a mandatory workflow whose bootstrap forces skills and brainstorming before plan mode. Anthropic's skills are demonstrations, mostly not about coding. | 10 of the 30 main tasks already bring their own AGENTS.md, CLAUDE.md or skills, which every arm loads. |

## Proposals

### A. Benchmark extensions: exploratory, after the pre-registered A0–A4 run

| # | Proposal | Size and cost | Records |
|---|---|---|---|
| A1 | **Skill bake-off.** Arms on A1's pane config, only the instruction text changing: K0 no style text, K2 Karpathy, K3 Ponytail plus Karpathy §3–4, K4 Ponytail plus three Pocock skills (`tdd`, `diagnosing-bugs`, `codebase-design`), K5 Ponytail plus three Superpowers skills without its bootstrap; K1 is A1, reused. Needs a `skills:` key for variants and a per-trial count of skill reads, so a trial shows whether a skill fired at all. | Stage 1: 10 main tasks × 1 rep × 5 arms = 50 trials, about $27 and 6.4 h. Stage 2, only after he sees stage 1: K0 and up to 3 survivors × 30 tasks × 3 reps = 360 trials, about $191 and 46 h. | new record; 0482, 0375, 0421, 0427, 0461, 0469 |
| A2 | **Lean tool roster** (`a1-lean`: read, edit, write, bash, grep): the playbook's and Destiner's anti-bloat claim, tested on omp itself. | +10 trials in stage 1, about $5 | 0422 ("no prompt diet"), 0482 |
| A3 | **Peak context per trial**, read from transcripts; a 100k compaction arm on long cases only if trials go past 100k. | metric: analysis only; arm M | 0448, 0399 |
| A4 | **Conditional rules**: Jocasta's own "Running tests" rules inside `<important if>`, Ponytail kept verbatim. | S | 0421, 0427, 0469 |
| A5 | **Post-hoc measures** from the full run's transcripts, no harness change: test-output bytes and reruns without an edit, piped test commands in review turns, type and lint escape hatches added, peak context. | analysis only | 0482 (labelled exploratory) |
| A6 | **Readings stated before the results:** A1 → A2 answers "plan mode is legacy", A2 → A3 "human code review is dying"; A5's interventions coded for brief loss (Loka's orchestrator problem). | none | 0482, 0285, 0453 |
| A7 | **Staged requirements**: follow-ups reveal requirements in stages, graded per stage, for maintainability. | M–L | 0482, 0423 |
| A8 | **Boss tools against a CLI**: the same scripted boss requests through MCP tools and through `jocasta boss <tool>`, as Loka measured. | M | 0387, 0389, 0465, 0484 |

Costs use the pilot's first medians ($0.53 API-equivalent and about 7.7 minutes of wall clock per trial, two at once) and are updated with the pilot's final figures. As in 0482, about 90 trials per arm detect only pass-rate differences of about 20 points, so claims about a skill set rest on lines changed, cost and time paired per task, with the pass rate read as non-inferiority.

### B. Product, after v1

| # | Proposal | Effort | Records |
|---|---|---|---|
| B1 | The reviewer gets his brief beside the approved plan, checking the diff against the intent and not only against the thread's reading of it; the brief is kept on the thread. | S | 0453 (item 2), 0425, 0459 |
| B2 | Optional `done_when` checks on `thread_start`, in the evals' check grammar, run in the review step. Reopens 0248 by its own trigger if the runs show threads reporting done with hidden checks failing. | M | 0248, 0453, 0473 |
| B3 | One task file per thread in the data folder (brief, approved plan, each review's proof, outcome), with the worklog's hash-marked blocks so his edits survive. | M | 0302, 0400, 0412, 0453 |
| B4 | The review proof also flags added escape hatches (`@ts-ignore`, `as any`, `biome-ignore`, `unsafe {`, `#[allow(`) and edits to check configs (`tsconfig*.json`, `biome.json`). | S | 0453 (item 3) |
| B5 | A thread can start from a fork of another thread's session (`omp --fork`), to try two designs from one plan. | M | 0388, 0399, 0412, 0285 |
| B6 | Typed herdr ids (`PaneId`, `TabId`, `WorkspaceId`), after the `ProcessId` precedent. Prevention only: no mix-up was found. | S–M | none |
| B7 | The review prompt asks for the test command unpiped, after the full run, so arm A3 matches the service during 0482's runs. | S | 0453 (a builder fix under 0363) |
| B8 | A loading rule: one always-on style text per pane, vetted technique skills on demand, no workflow frameworks (Superpowers' bootstrap collides with plan mode, worktrees and `thread_finish`). A1 settles it. | S | 0427, 0461, 0465, 0285, 0426 |

### C. Platform, after v1

| # | Proposal | Effort | Records |
|---|---|---|---|
| C1 | **omp²:** stay on 18.x through A5 and the cutover (0500 already says so) and write an omp² break list into plan §5.5. When omp² ships, the move is its own record (cfg overlays, an `.oms` reader, a herdr hook, `--plan-mode`), and later a session-stream client replaces tailing files. | S list; L move | 0500, 0383, 0386, 0450, 0485, 0464 |
| C2 | **herdr-core:** keep 0.9.5; a spike maps `herdr.ts` onto `hc` (workspaces and tabs become Jocasta's state, `worktree.create` becomes `git worktree add`, `agent.prompt` becomes `terminal.send`, agent states come from OSC 7501, which omp already emits); re-check when `hc` leaves preview. | M spike; L port | 0383, 0392, 0412, 0435 |
| C3 | **Companion:** measure before any rewrite: H6 split into the Bun service, the WebView2 processes and the whiteboard server; H5 time to first paint; a bare WebView2 host as the floor. 0496's follow-up has nothing to switch: Electrobun 2.0.2 never sets `AllowSingleSignOnUsingOSPrimaryAccount`, whose default is false, so the peer is likely the runtime's own [INFERENCE]. An egress check of the bare host settles it, and Tauri uses WebView2 too. For later: a native rail (GPUI or egui, or a herdr-app mod after C2) with the canvas kept on the web. | S measures; L rail | 0367, 0368, 0393, 0409, 0496 |
| C4 | **Boss tools as a proxy boundary:** sort them in appendix I (9 boundary, 8 view or convenience); OS isolation's acceptance test becomes "no script a thread writes reaches the service's token or `/api/mcp/*`". Also an inventory of which seams would hold on Pi. | S; the test is part of the L isolation work | 0484, 0485, 0386 |

## For the thesis (no product change)

- **Chapter 2, related work:**
  - HumanLayer's open-source release is Jocasta's closest open-source relative: an orchestrator over unforked harnesses, with plans and review. It makes the opposite deployment choices: cloud and teams against local and single-user.
  - Also: the playbook, beside [40] "The Harness Problem" by the same author; Loka; Destiner and Pi; herdr's own split.
- **Chapter 3, design rationale:**
  - 0386 reads as "an opinionated engine, kept outside our code", which answers Destiner.
  - herdr's "Core draws nothing" independently mirrors 0392's split: a server owns the terminals, and the UIs are its clients.
  - The playbook's five consequences serve as the lens: one authority, a trusted control plane, bounded work, explicit compatibility, views as projections.
- **Chapter 5, evaluation:**
  - The pipe fix is a measured case of a model's self-report weakening a deterministic check.
  - Record-to-test traceability: a read-only script in `D:/thesis/analysis/` gives the exact share of records a test cites.
- **Chapter 6, discussion:** Loka's bypass is 0485's accepted risk; A6 answers Destiner's two claims; HumanLayer's 100k stands against 0448's 300k.
- **Chapter 7, future work:** B1–B8 and C1–C4.

## Not verified

- **omp²:** its proposal shape, the `usage` and `auth-gateway` formats, and Windows support.
- **The likely omp² breaks:** herdr's integration and `APPEND_SYSTEM.md` come from a search of the branch, not a run.
- **Sources known only at second hand:** Karpathy's own post (X blocks automated reading); the ETH Zurich, Chroma and SlopCodeBench studies HumanLayer cites; `run_silent.sh` (404).
- **The 0496 peer:** whether OneAuth is its source, and whether a bare WebView2 host shows it.
- **omp's shell minimizer:** which test runners beyond pytest it compresses.
- **Superpowers' Pi extension:** whether it loads in omp.
- **Estimates:** token sizes are characters ÷ 4, and the test count comes from a regex over test names.

## Decided 2026-11-01 (records follow)

- **Full run:** go, with three fixes first: the trial branch named `eval/trial-<rep>` (no task id, so no PR number), a per-trial `lines.log`, and a 90-minute limit for every arm.
- **Finding 2:** session state read along omp's active branch, built before the cutover.
- **Skill study:** both stages, with three more arms in stage 1: the lean tool roster, conditional rules, and the 100k compaction point.
- **After v1, product:** B1 (the reviewer reads the brief), B2 (`done_when` checks), B3 (a task file per thread), B4 (escape-hatch flags).
- **After v1, platform:** C1 (the omp² break list, written now), C3 (measure the companion first), C4 (the boss tools as a proxy boundary). C2 (herdr-core) wasn't clear and is explained below.
- **Small items:** B7 (an unpiped test command in the review prompt, after the full run), A5 (post-hoc measures), A6 (readings stated before the results).
- **Not chosen:** B5 (a thread started from a fork) and the CLI comparison for the boss (A8, not asked).

## Per source: discussion, suggestions, questions

Each source: what it argues, what Jocasta already does or decided, the case against, and what's still open. The full quotes are in the six reports.

### Stencil, "The Harness Playbook" (C. Bölük, 2026-09-02)
- **Argues:**
  - State must come from one journal: "If authoritative state cannot be derived from the journal, rewind, fork, and resume are lies."
  - Policy stays on a trusted host, and sandboxes get bounded requests.
  - "Every permanent tool taxes every turn."
  - The agent must not be able to redefine success.
  - Start compaction "~10% before the limit".
  - Plan and goal modes become composable "Directors".
  - TypeScript is "an awful choice" for an engine, hence omp² in Rust with Python extensions.
- **Jocasta:** finding 2 (branch-aware state) is the playbook's own failure case, fixed before the cutover; omp²'s break list goes into the plan; the lean-tool arm tests the tool tax. It already keeps views as projections, a kill boundary and proof before done.
- **Against:** a vendor's post about its own unreleased product; only the tool-roster numbers come with data.
- **Open:** the five consequences as the thesis's lens (ch. 3, 6); a session-stream client after omp² ships; the plan gate as a Director (omp² only).

### Loka, "Stop writing tools" (P. Kalinovski, 2026-10-20)
- **Argues:** tools are rails for weak models. AGENTS.md, skills and CLIs beat nine agents with forty tools ("116 → 25" tool calls, "≈$7.35 → $0.88", quality "comparable", the author's own runs); agents-as-tools lose context; scripts bypass safeguards, so credentials belong behind a proxy.
- **Jocasta:** the boss's tools are that proxy (the service's token, the boss key, Allow/Deny), and C4 writes the boundary down. A6 codes A5's interventions for lost context.
- **Against:** single runs by the author; boss requests are short, so there is less to save; Pi's makers kept MCP for structured results.
- **Open:** measure the boss through a `jocasta boss <tool>` CLI against MCP before changing anything (A8), or leave the tools as they are.

### Destiner, "Pi is the last harness" (2026-11-01)
- **Argues:** Pi is the only open, vendor-agnostic, extensible and minimal harness; "bloat especially harms the frontier models"; "'Plan mode' was a thing; now it's legacy. Human code review is dying."
- **Jocasta:**
  - 0386 chose the opinionated engine (whole omp), because 6.4K of the old harness's 8.9K lines re-implemented omp's features.
  - It agrees on one engine with Jocasta's code outside it.
  - The lean-tool arm and A6's readings test the claims.
- **Against:** opinion without data, and Pi leaves out plan mode, subagents and permission prompts, which Jocasta would rebuild.
- **Open:** an inventory of which of Jocasta's seams would hold on Pi; his own view on "plan mode is legacy" and "review is dying", for 6.3 as his opinion.

### HumanLayer, "Context-efficient backpressure" (2025-12-09)
- **Argues:** checks should be silent on pass and complete on fail; models filter test output with `head`/`tail` and then rerun the suite.
- **Jocasta:**
  - The piped-test fix is built, and B7 asks for an unpiped run after the full run.
  - omp already compresses output and spills anything over 50 KB to a file.
  - Ponytail says to save test output to a file.
- **Against:** omp's minimizer already does much of this; how many tokens test output still costs is unmeasured (A5 measures it).
- **Open:** a quieter-output arm only if A5 shows a real load; a minimizer config for the test runners omp may not cover (bun, vitest, jest, go test: unverified).

### HumanLayer, "Getting Claude to actually read your CLAUDE.md" (2026-03-17)
- **Argues:** `<important if="…">` blocks gave "noticeably better adherence" ("We don't have a rigorous explanation"); keep the file short ("under 60 lines").
- **Jocasta:** the conditional-rules arm tests it. Ponytail plus Jocasta's test rules run 68 lines; the boss manual is 63 lines and 1,524 words, rare procedures already in skills (0465).
- **Against:** anecdotal, and about Claude Code's system reminder; omp puts context files in a `<project-context>` block.
- **Open:** if the arm gains, the boss manual's rarely relevant paragraphs get the same blocks; or a size budget for always-on text.

### HumanLayer, "Skill issue: harness engineering for coding agents" (2026-03-12)
- **Argues:** success tracks "the agent's ability to verify its own work"; a Stop hook runs the checks silently and "exit code `2` tells the harness to re-engage the agent"; subagents work as a "context firewall"; persona subagents "don't work".
- **Jocasta:** the review turn checks at the end of a task; the boss is the context firewall ("never a thread's content"); there are no persona subagents.
- **Against:** Jocasta can't hook inside omp (0386): anything like a Stop hook has to run from the service.
- **Open:** the service runs the project's check (`mise run verify`, 0473) when a thread goes idle with new edits and sends only the failures into the pane, silent on pass.

### HumanLayer, "Long-context isn't the answer" (2026-03-23)
- **Argues:** "Long-context models degrade at all context lengths"; "the instruction budget doesn't scale"; their warning moved to 100k tokens on 1M windows.
- **Jocasta:** compaction at 300k (0448, which set the older 40% advice aside); the 100k arm and the peak-context measure test it.
- **Against:** measured on Opus 4.6; Jocasta runs Opus 5.5; earlier compaction loses detail and prompt-cache hits.
- **Open:** an earlier compaction point for the boss pane, which lives longest, once the arm reports.

### HumanLayer, "Context forking" (2026-05-15)
- **Argues:** fork a conversation to correct course, to explore designs from one rich context, or to drop a burst of noisy context.
- **Jocasta:** omp has `/fork`, `--fork` and `/tan`; threads start fresh. B5 (thread from a fork) was not chosen.
- **Open:** only a boss manual line: when he wants two designs, two threads get the same brief, with no fork.

### HumanLayer, "show-me" (2026-08-12)
- **Argues:** agents should "converse visually instead of in walls of prose": call trees, file trees, diff-shaped sketches, HTML.
- **Jocasta:** the whiteboard canvas and the boss's whiteboard-review skill; voice replies stay prose.
- **Open:** show-me's formats (MIT) in the whiteboard-review skill; a text-only show-me skill for thread panes.

### HumanLayer, "All resources" (2026-09-04)
- **Argues:** "one-shot tests, token use, and task count do not measure whether code stays easy to change".
- **Jocasta:** the task benchmark is one-shot; this is a limitation of chapter 5.
- **Open:** a staged-requirements eval after the full run (A7), or the limitation stated in the thesis only.

### HumanLayer, "Open sourcing HumanLayer" (2026-10-23)
- **Argues/offers:**
  - An Apache-2.0 Bun/TypeScript monorepo: a macOS desktop app, a daemon over Claude Code, Codex, OpenCode and Pi, and a cloud API.
  - "no telemetry at all" when self-built.
  - "a lot of third party API keys that could be made optional, but are not yet".
- **Jocasta:** not a base: it conflicts with 0386 (omp whole), 0392 (herdr) and 0478 (local-only), and it targets macOS and teams. It is the closest open-source relative.
- **Open:** the thesis's comparison (chapter 2: same category, opposite deployment choices); its phase auto-advance table as a pattern for the threads' phases.

### Matsakis, "What agents mean for Rust (and vice versa)" (2026-10-14)
- **Argues:** "Writing tests has never been enough … with agents, it's not even close"; newtypes strengthen guardrails; review the interface apart from the implementation; designs should turn into checks.
- **Jocasta:** B4 flags escape hatches in review; `ProcessId` is the repo's one newtype; strict TypeScript.
- **Against:** no known id mix-up in Jocasta, so newtypes would be prevention only; no rewrite (0009).
- **Open:** typed herdr ids (B6); the reviewer reporting changes to exported interfaces separately.

### Typemill, "Tasks as code" (2026-10-25)
- **Argues:** "The task description, implementation plan, and report remain together in one file"; a person reviews the plan first; private tasks live apart from the code.
- **Jocasta:** B3 (a task file per thread, in the data folder) carries it; plan approval is the gate.
- **Open:** `/handoff` reads the task file; the boss turns inbox notes into task files he can edit (a backlog).

### Miessler, "Writing is closer to thought than code" (2026-10-23)
- **Argues:** most failures are unclear asks; decisions are written down "with the date and the reason"; each criterion carries its own check; work is checked "against the ISA instead of against the last drawing".
- **Jocasta:** decision records before code (0002), 314 of 507 records amended; B1 and B2 carry the rest.
- **Open:** a record-to-test traceability number for chapter 3.3; a "Checked by:" line in the record template (0031).

### herdr's split (herdr-core and herdr-app, October 2026), in plain terms
- **What changed:**
  - herdr is the terminal program Jocasta runs everything in: the boss pane, a tab per project and per thread.
  - Its makers started two new programs: `herdr-core` (`hc`), a background server that owns terminals and draws nothing, and `herdr-app`, a desktop window (Rust, GPUI) whose panels can be added by small JavaScript "mods".
  - The herdr Jocasta uses (0.9.5, now called "Herdr TUI") keeps working, but it doesn't share agents with the new programs.
- **Why it matters:**
  - `hc` has no workspaces, tabs or worktrees, so moving Jocasta onto it means Jocasta keeps those itself.
  - If the old herdr stops getting fixes, that move becomes necessary.
  - herdr-app could later host Jocasta's rail as a native panel; mods can't show web pages, so the canvas stays web.
- **The "spike":** a short investigation that maps every herdr call Jocasta makes onto `hc` and lists what's missing, so the cost is known before any decision. Nothing moves.
