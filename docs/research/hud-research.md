<!-- Research of 2026-11-23 for records 0667-0673: omp and pi activity, the pi and omp extension ecosystem and orchestrator dashboards, and the omp community (its Discord needs a login; GitHub Discussions and issues read instead). Written by three research agents from public sources; their [INFERENCE] marks stand. -->

# HUD and orchestration research (2026-11-23)

# UpstreamTrends: omp + pi activity, 2026-10-24 → 2026-11-23

Sources (all seen 2026-11-23 via `gh`): omp releases https://github.com/can1357/oh-my-pi/releases (v18.14.0 → v19.1.1), omp PRs/issues https://github.com/can1357/oh-my-pi/pulls , pi repo https://github.com/earendil-works/pi (badlogic/pi-mono redirects there; releases v1.5.0 → v1.6.0), omp `packages/coding-agent/CHANGELOG.md` Unreleased section (raw from the GitHub API).
Everything marked [INFERENCE] is my reading, not stated by maintainers. PR/issue numbers are in `can1357/oh-my-pi` unless prefixed `pi#`.

## 1. Findings

### 1.1 Release cadence (verified)
omp ships ~1 release/day: 18.14.0 (10-29) … 18.22.2 (11-18), **19.0.0 (11-19), 19.0.1 (11-19), 19.1.0 (11-21), 19.1.1 (11-23)**. 19.0.0 is a breaking major (see §2). pi is slower: 1.5.0 (11-02), 1.5.1 (11-09), 1.5.2 (11-10), 1.6.0 (11-20). pi merges only maintainer/trusted-contributor PRs (new-contributor issues are auto-closed), so almost all new capability lives in omp.

### 1.2 omp: notable shipped features by theme

| Theme | What shipped | Where |
|---|---|---|
| **Approvals in session file** | `tool_approval_requested` / `tool_approval_resolved` custom entries are written to the session JSONL, request on disk *before* the prompt opens, no extension needed. Documented in `docs/session.md`. Made for "multiplexer sidebars/dashboards". | PR #18837 (merged 11-20, in 19.1.0) |
| **Daemon** | 19.0.0 consolidates all background processes (MCP servers, LSP, PTYs, services) under **one per-user daemon**; survives omp upgrades; optional TCP via `OMP_DAEMON_TCP`; `omp ps` reports global/project/session units, `--json` is now `{daemon, reports}`, `--global` removed. 19.1.0 moves MCP servers into the daemon (`mcp.shared`, `mcp.idleStopMinutes`). Open: daemon memory diet #20224 (merged, Unreleased). | 19.0.0, 19.1.0 notes |
| **Cross-instance agents** | `task.crossInstance` exposes same-folder omp instances at `agent://peers` + messaging. | 19.0.0 |
| **Subagents / task** | `task.steps` ordered multi-step tasks (18.19.0); `task.autoParallel` off/suggest/on (18.19.0); `task.agentPrewalkAuto` (18.21.0); removed `task.file`/`taskFile` (18.19.0, breaking: instructions inline only); per-call `model` (#12229 merged), role selectors open (#19921); Tern live cards for running subagents + restored finished agents (19.0.0); waits follow prompt-cache lifetime; wait ladder = 5 s first, then 4 min. | releases above |
| **Run lifecycle (SDK, breaking)** | `agent_end` fires once when a run can no longer resume; new `agent_pause`; `hasPendingAsyncWork()/settleAsyncWork()` replaced by `isRunActive`, `wakeSources`, `waitForRunEnd()`; `abort()` ends runs paused on background work. RPC/JSON print/collab streams *keep* per-loop `agent_end`. | 18.21.0 |
| **Cost / caching** | Cache warming strategies `auto/keepalive/compact` + no-output Anthropic `prewarm`; SDK `cacheWarming: "off"|"streaming"|"idle"` (was boolean, breaking); cache-warm events report method + cost; `cache` status segment; Anthropic caching preserved when tools activate late (#19826); GPT-6.2 prompts moved out of uncached `instructions`; `/usage` shows Claude plan (Max 5x/20x…); `AssistantMessage.requestId`; `omp bench --account`. Opt-in prompt perturbation (per-install paraphrase of system prompt) in 19.1.0. | 19.0.0 / 19.1.0 / 18.17.0 |
| **Compaction** | `session.handoff()` → `session.compact(focus,{mode:"handoff"})`; `/handoff` is background compaction; open PR #20286 global `compaction.model` (cheap summarizer); open #19820 idle compaction for RPC mode. | 19.0.0; PRs |
| **Remote / collab / RPC** | 19.1.0: "remote session model controls through RPC … collab guests can continue, interrupt and queue follow-ups for the host"; **RPC hosts must handle `ask` as ONE complete HTML-`form` request** (breaking); 19.0.1 collab fix for subagent-progress backlog (#19710); `omp --resume <id>` under `--mode rpc/rpc-ui/acp` works when the session cwd is gone (#19651); 11-23 fix: RPC `config_warnings_changed`, `advisor_*` events now delivered (#20254). Open #20272: RPC `tool_execution_update` heartbeats (`partialResult:{heartbeat:true,elapsedMs}`) for silent tools, default 30 s. | releases/PRs |
| **MCP** | Protocol 2026-07-28 support; MCP built-in transforms; `mcp.evalNamespaces`; servers run in daemon; standalone MCP transport/tool-cache SDK exports **removed** (breaking: use MCP host APIs); open #20270 MCP form elicitations answered from session UI (TUI, `rpc-ui`, ACP only; not `-p`). | 19.0.0 / 19.1.0 |
| **Tool surface** | `xd://` devices replaced by "tool commands": rare tools run as `<tool> --help` / `<tool> <args>` inside `bash`; settings renamed `tools.xdev*` → `tools.commands*`; docs `xd://` → `help://`. `plan`/`propose`, `resolve`, `reject`, `report_issue` are now commands. | 19.1.0 (breaking) |
| **Guards / hooks** | TTSR `gate: before` rules (#19893 merged) judge tool calls; open #20216 adds `command:` probes (exit code decides; user-level rules only); open judge-backed tool guard #19903, `toolInterceptor` #19642; bash `notify` option; `bash.patterns` path-scoped, and **`--config` overlays now *prepend* rules to global rules** instead of replacing (18.20.3). | releases/PRs |
| **Extension APIs** | `registerProvider` can replace `stream/generateImage/webSearch/discoverModels`; `pi.registerTodoBackend()` + `todo.backend`; `excludeFromContext` on `pi.sendMessage`; `parentToolCallId` on `tool_call`/`tool_result` events; `registerTranscriptPresenter` (18.17.0). | 19.0.0 / 19.1.0 |
| **History / archive** | `archive.search` now full-text over all sessions and returns session records (breaking; `archive.prompts({query})` for prompts); Unreleased adds `archive.calls/grep/stats/messages` + `since/until` — i.e. tool-call counts/errors by tool/day/model/project from session files. Open #20035 `history://session/browse`. | 19.1.0 / Unreleased |
| **Worktrees** | `/wt` runs `post-checkout` hook (19.1.1); ovnfs checkouts in `/wt`, `omp worktree list/clear`; `worktree.onExit: Force Remove`; open #19283 `.worktreeinclude`; open #19924 session header records `gitBranch`. | releases/issues |
| **Stats** | stats dashboard 2d/3d presets; open #20294 indexes approval prompts into `stats.db` (`tool_approvals` table). | 19.0.1 / PR |
| **UI** | Tern (native host) is the main UI investment: live cards, plan-review editing + comments, model switcher, ask form redesign; Vim mode; composer predictions on by default; `/btw` notes; opt-in `tree` status segment; Chrome side panel + native-host relay (breaking reinstall of Chrome extension). | 18.22.0, 19.x |
| **Providers** | Kiro native, Modal, Devin accounts, Z.AI discovery; **Gemini CLI OAuth provider removed** (19.1.0); Antigravity paused by design (#18499/#18341 — ToS). Not relevant to Jocasta unless it routes via those. | releases |

### 1.3 pi (earendil-works/pi) notable
- **1.5.1** idle prompt-cache warming (up to 30 min, default `cacheWarming: "idle"`), Anthropic "message threads" (server-side continuation; send only new messages), `pi auth login` from shell, `pi.registerMarkdownTransformer`, extension changelogs (1.5.0), Claude Fable 5.2.
- **1.6.0** GPT-6.2 Astra/Sol default; extension `render()` returns `readonly string[]` (breaking); find-tool patterns relative to search dir (breaking-ish); session-start/RPC fixes: `new_session/switch_session/fork` no longer double-fire `session_start`; `AgentSessionRuntime.takeUnreportedDiagnostics()`; `pi.exec` returns 128+signal; RPC `ctx.shutdown()` honoured when idle (1.5.1).
- Merged PRs of note: pi#12421 `pi-classifier` API, pi#12356 JSON mode exits nonzero when final response fails, pi#10703 durable: extensions annotate aborted tool results, pi#11255 compaction model + thinking level configs (the same idea omp has open as #20286), pi#10663 `pi auth --continue`.
- Direction: pi = lean core + provider breadth + caching correctness; no subagents/daemon/collab. [INFERENCE] pi will stay a *source of provider/caching fixes* that omp later ports (e.g. omp's cache warming/message threads are pi-1.5.x-era ideas).

## 2. Trends — what maintainers are building toward, and predictions

| # | Trend | Evidence | Prediction (≈ next 2–6 weeks) [INFERENCE] |
|---|---|---|---|
| T1 | **Daemon-centric omp**: one per-user daemon owns MCP, LSP, PTYs, services; TCP option; survives upgrades | 19.0.0 breaking note; 19.1.0 MCP-in-daemon; #20224; open #19677/#19628/#19554 daemon hardening issues; #5742 (GUI on top of omp, open since July, maintainer says "daemon contract" is the real work) | Daemon grows a **session registry** (list running sessions, attach) → cross-process agent list. omp itself will own "what is running" without a Jocasta layer. |
| T2 | **Multi-session / agent overview** | Open #19934 `omp agents` background sessions + Agent View (11-20, community PR, "Claude Code–style"); #1627 Agent View, #8656 multi-session in one process, #8077/#7255 cross-session messaging (all open, triaged `enhancement`); shipped `task.crossInstance` + `agent://peers`; Tern tray cards | A built-in agent view lands (maintainers triaged all variants as wanted but undecided on design). Ranked likelihood: #19934 or a maintainer rewrite in 19.2–19.4. |
| T3 | **Machine-readable "waiting on user" state** | #18837 approvals in JSONL (merged), #20294 stats index, #18803 (`ctx.resolveToolApproval`/`getPendingToolApprovals`, open, for phone/remote approvers), #18934 task-completion notifications distinct from turn completion (open), 19.1.0 bash `pty` input-waiting notifications, #19407 (RPC approval identify, closed) | `resolveToolApproval` in some form, probably as an RPC command or extension ctx method; more `custom` entries for run state in session file. |
| T4 | **Deterministic guards instead of model calls** | #20216 `command:` probes for `gate: before`, #19893 gate rules, TTSR repeatMode, bash.patterns path-scoped, `--no-judge/--offline` flags (18.20.3); counter-trend: #19903/#19642 judge-backed guards (opt-in) | Both exist; deterministic one is cheaper and gets merged first. Same philosophy as Jocasta's guard — expect overlap. |
| T5 | **Cache/cost economics are first-class** | cache warming modes + prewarm + events with $ (19.0.0); `cache` status segment; `compaction.model` #20286; idle compaction #19820; prompt perturbation (19.1.0); `AssistantMessage.requestId`; Claude plan in `/usage`; `auth.accountPolicies`; open #19623 (record why OAuth account switched; RPC `account_switched` event); #19134 per-session account pool | More cost data in events (`account_switched`, cache-warm cost) and settings to route cheap work (compaction/judge/title/subagent roles) to cheap models. |
| T6 | **SDK/RPC surface is being widened and broken in majors** | 18.21.0 run-lifecycle change; 19.0.0 `cacheWarming`, `handoff`; 19.1.0 `ask`→form, MCP exports removed, `archive.search`; #20254 missing RPC events; #20272 heartbeats | Expect more breaking RPC/SDK changes on 19.x; each minor needs a Jocasta contract test. |
| T7 | **Orchestration inside omp** | `task.steps`, `task.autoParallel`, `omp workload` YAML DAG runner (#19923 open: "deterministic, re-runnable from cron/CI"), #19921 per-call model roles, #19790 spawn depth, #19901 judge-watched background jobs, goal/vibe mode (#19317) | `omp workload` (or equivalent) lands; Jocasta's thread pipeline will partly duplicate it. |
| T8 | **Tern / native host + browser/side-panel** | 18.22.0, 19.0.0, 19.1.0 Tern/side panel items; Chrome extension native host | Tern-first UX features arrive before TUI; terminal-only users (Jocasta) get fewer UI gains. |

## 3. Impact on Jocasta, by seam

Legend: **DROP** = Jocasta can delete code it builds; **CHEAP** = enables a cheap feature; **BREAK** = risk to a seam; **WATCH** = no change yet.

| Item | Class | Why / what to do |
|---|---|---|
| `tool_approval_*` entries in session JSONL (#18837, 19.1.0) | **CHEAP + DROP** | Jocasta's today dock can show "thread blocked on approval" by tailing session files — deterministic, no model, no RPC attach. Likely replaces any approval-state heuristic Jocasta keeps (e.g. scraping pane text/herdr state). Verify entry shape in `docs/session.md`; fields: `toolCallId, toolName, approvalMode, reason?, requestedAt` / `approved, resolvedAt`. Pending = request without a later resolution or tool result for same `toolCallId`. |
| `archive.calls/stats/messages` + `since/until` (Unreleased; `archive.search` records, 19.1.0) | **CHEAP / possible DROP** | Jocasta's `history_search` and parts of the cost ledger may be answerable by omp's own `archive` (tool calls/errors by tool/day/model/project). It runs *inside* omp eval, so Jocasta would call it through a boss tool, not from Rust. Check before building more history features. BREAK risk: `archive.search` return shape changed in 19.1.0 (session records, begins-of-word match) — verify `history_search` still works on ≥19.1. |
| `omp stats` DB, #20294 `tool_approvals` table | **CHEAP** | A read-only SQLite (`stats.db`) is a deterministic source for tool/approval/cost stats instead of parsing JSONL. [INFERENCE] schema may churn; pin a version check. |
| `omp ps --json` shape `{daemon, reports}` (19.0.0) | **BREAK** if Jocasta parses it | Old project brokers not adopted; `--global`, `?project=` removed. Check for any `omp ps` use. |
| Daemon consolidation / `OMP_DAEMON_TCP` | **WATCH / CHEAP** | One daemon per user means MCP servers shared across threads (`mcp.shared`) — each thread omp no longer spawns its own MCP children (RAM/token-free win for Jocasta's MCP config). Risk: unreachable-daemon failures (#19677, #19628: unreadable daemon log breaks list RPC; shared mounts) — Windows install should be fine but watch. |
| RPC `ask` now a single HTML-form request (19.1.0, breaking) | **BREAK** if Jocasta drives RPC mode and handles `ask` | Field-by-field prompts no longer emitted; host must answer one `form`. Jocasta mostly uses `--continue`/print/session files — verify the boss path does not use `--mode rpc`. |
| Run lifecycle: `agent_end` once, `agent_pause` (18.21.0) | **BREAK/WATCH** | Anything that treats `agent_end` as "thread finished" in SDK/extension code changes meaning; RPC/JSON-print/collab streams retain per-loop `agent_end`. Print mode `-p` unaffected by description. Also session JSONL: [INFERENCE] a paused-on-background run no longer looks "done"; check Jocasta's thread-state detection with `wait`/background jobs. |
| `--config` overlays: path-scoped `bash.patterns` now *prepended* to global rules (18.20.3) | **BREAK-ish / CHEAP** | Jocasta's guard rules via `--config` overlays now stack in front of the user's rules rather than replace — verify precedence on 19.x. Positive: `bash.allowedEnvVars`, per-rule `env`. |
| `--continue` / `autoResume` refuse a session being written by another omp process (19.1.0 fix) | **BREAK-ish** | Jocasta's `--continue` flow on a thread session that is still held open (another omp process alive) will now not attach. Make sure the thread's previous omp process has exited before `--continue`; use explicit `--resume <id>` otherwise (and note #19651: `--resume` works under rpc/acp when cwd deleted). |
| `task.file`/`taskFile` removed (18.19.0) | **BREAK** if Jocasta writes task files for subagents | Instructions must now be inline. |
| `task.steps`, `task.autoParallel`, per-call `model` (#12229), #19921 roles | **CHEAP** | Boss/thread prompts can ask omp to sequence/parallelise instead of Jocasta doing it. Deterministic scheduling stays in Jocasta; only adopt if it saves tokens. |
| `compaction.model` (#20286, open), idle compaction (#19820) | **CHEAP (token)** | One config line in the `--config` overlay → cheap summarizer for long thread sessions. Not merged yet; global-only (project settings cannot set it) — Jocasta's overlay is `--config`, [INFERENCE] likely counts as operator layer; confirm after merge. |
| Cache warming (`cacheWarming`, `idle`/`streaming`, `cacheWarming:"off"` in SDK) | **COST-relevant** | Warming spends tokens by design ("only when refresh pays for itself", pi 1.5.1; omp events report $). Jocasta's cost ledger should ingest these events/entries or its totals drift. For idle boss session on API billing check the default. [INFERENCE] subscription users unaffected. |
| `AssistantMessage.requestId`, usage metadata, plan in `/usage` | **CHEAP** | Ledger can store Anthropic `request-id` per message for support/audit. |
| `account_switched` (#19623, open), `auth.sessionAccountPools` (#19134, open) | **CHEAP later** | If Jocasta runs multi-account, this gives a deterministic "why did cost jump / cache miss" line. |
| Collab: guest can continue/interrupt/queue (19.1.0), backlog fix (19.0.1), `collab-web` perf | **CHEAP / DROP?** | Jocasta uses the collab relay for the whiteboard/review page. Check whether new remote model controls + review/plan editing in Tern/collab overlap with whiteboard. Seam stable (additive), but 19.1.0 "RPC hosts must handle `ask` as form" also affects collab guests. |
| Plan review in Tern (editable, comments → Refine) 18.22.0; plan approval via `propose` command (19.1.0) | **BREAK/WATCH** for plan mode | Plan submission is now a `propose` *command* inside `bash` (tool-commands, `help://propose`), not an `xd://` device. Jocasta's plan-mode gate that detects plan submission by tool name must follow: check tool-call name in session JSONL for `propose` (inside bash) vs old `exit_plan_mode`/xd. Test on 19.1.x. |
| `xd://` → tool commands; `tools.xdev*` → `tools.commands*` | **BREAK** for config overlays | Any Jocasta `--config` overlay or guard rule referencing `tools.xdev*` or `xd://` is dead on 19.1.0. Also guard must now see rare tools as *bash commands* (`ast_edit …`, `github …`, `lsp …`) — a bash-pattern guard will match them. |
| MCP SDK exports removed; MCP in daemon; `mcp.shared` | **WATCH** | Jocasta uses MCP (graph_search, etc.) as a config entry, not via SDK exports; fine. Shared daemon could reduce per-thread startup time. |
| `task.crossInstance` / `agent://peers` | **CHEAP** | Boss omp could see peer thread agents without Jocasta's own registry — but Jocasta keeps deterministic thread state (worktree, gates). Use only for read-only discovery. |
| `/wt` + `post-checkout` hook + `worktree.onExit` | **WATCH/overlap** | Jocasta creates worktrees itself; omp's `/wt` is user-facing. No conflict; `worktree.onExit: Force Remove` could silently delete a thread worktree if set globally — ensure Jocasta overlay pins `worktree.onExit` or doesn't rely on omp-created worktrees. |
| Session header `gitBranch` (#19924, open) | **CHEAP** | Map session → branch/worktree without asking git. |
| `history://session/browse` (#20035, open) | **WATCH** | Overlaps Jocasta `history_search`; maintainers undecided. |
| `omp workload` (#19923 open) | **WATCH / possible partial DROP** | If merged, a YAML DAG of subagent+shell steps from cron/CI is the same shape as Jocasta's gated pipeline. Don't build more pipeline-orchestration until decided. |
| TTSR/`gate: before` `command:` probes (#20216 open) | **CHEAP / overlap with Jocasta guard** | If merged, omp itself can run a user-level deterministic shell probe before tool calls (exit code decides). Jocasta's guard (approval rules on shell commands) overlaps; consider routing Jocasta rules into omp as user-level TTSR rules so they apply inside subagents too. Note: only user-level rules may run `command:`; project-level/`.omp/rules` are refused — a Jocasta overlay would need to be user-level. |
| Gemini CLI OAuth provider removed (19.1.0); Antigravity paused | **BREAK** only if Jocasta uses those providers | Otherwise ignore. |
| Intent tracing off by default in print/RPC/ACP/SDK (19.0.0) | **WATCH** | If any Jocasta feature uses tool-call `intent` fields from sessions (status text), they're gone by default; enable `tools.intentTracing` in overlay or stop relying on it. |
| Print mode (`-p`): retry window for auth broker restarts (30 s); retries printed with error (18.16.0) | **WATCH** | stdout of `-p` may now contain retry lines in text mode; use `--mode json` for machine parsing. |
| pi core | **WATCH** | Jocasta drives omp, not pi. Fixes land in omp only after ports. |

## 4. Feature requests in issues an orchestrator could serve better than the harness

| Issue | Ask | Why an orchestrator fits | Jocasta angle |
|---|---|---|---|
| #1627 (open) Agent View — multi-session parallel monitoring; #19934 (PR) implementation | list N running sessions with state, detach/reattach | Needs process-level supervisor, not one agent; maintainers called omp's registry "process-global only" | Jocasta already is this. The `today` dock thread list + approval-wait state (from #18837 entries) = exactly the missing view. Strong positioning. |
| #8656 (open) many live session threads in one process; #8077/#7255 cross-session messaging | inter-session channels | Orchestrator is the natural hub (boss ↔ threads) | Keep Jocasta as router; no need to wait for omp. |
| #5742 (open) GUI on top of omp; #6171 collab-web global remote toggle | web/desktop UI, remote approvals | Jocasta whiteboard + herdr panes | Already partly covered; #18803 shows people want remote approvals (phone companion) — a Jocasta notification hook that reads `tool_approval_requested` can do that with zero model calls. |
| #18803 (open) `ctx.resolveToolApproval` for extension | answer approvals remotely | Orchestrator-level approval policy (guard) can decide centrally | Jocasta guard could auto-approve/deny per rule once omp exposes it; until then Jocasta guard sits in front via overlay rules. |
| #18934 (open) task-completion notifications independent of turn completion | "thread done" ≠ "turn done" | Orchestrator knows task done = gates passed + merged | Jocasta thread lifecycle already models this; expose as dock/notification. |
| #18485 (open) "add back old `wait`" — wait ladder burns tokens as models poll (DeepSeek Flash example) | token waste from polling | Orchestrator can wake agents on events (session file tail, deterministic) instead of model polling | Jocasta advantage: do not let boss poll threads; push events. Matches standing preference for determinism. |
| #19283 (open) `.worktreeinclude` when clone-first falls back to plain checkout | copy gitignored files into worktree | Worktree creation is Jocasta's job | Jocasta worktree setup could honor `.worktreeinclude` (S effort, deterministic). |
| #18219 (open) MCP tools ignore `readOnlyHint` for approval (every read-only call prompts in `write` mode) | fewer approval prompts | Guard layer can allow read-only MCP deterministically | Jocasta guard rule: allow MCP tools by annotation/allowlist. |
| #19134 / #19623 (open PRs) per-session account pinning and "why did account switch" | cost/cache surprises on account rotation | Cost ledger | Show switch reason in ledger when merged. |
| #20286 / pi#11255 (cheap compaction model) | cost | Config line | Add to overlay after merge. |
| #19923 (PR) YAML workload runner; #19921 per-call role models | re-runnable pipelines, cheap-vs-expensive model per step | Orchestrator job | Overlap risk: see §3. |
| #19901 (PR) judge watches stalled background jobs | "are you stuck?" | Orchestrator can do this deterministically (no model): job still alive + no output N min | Dock "stalled thread" badge from session mtime + heartbeat (#20272) — S effort. |
| #17444, #12471/#20270 | MCP elicitation, external providers | MCP servers that need human answers | Only works in TUI/rpc-ui/ACP, not `-p`; Jocasta print-mode threads cannot answer elicitations. |

## 5. Ranked feature ideas for Jocasta (from upstream signals)

Effort S/M/L. "Data omp already writes" = reads session JSONL/events/DB; "New model call" = new LLM spend.

| Rank | Idea | What | Evidence | Lives in | Effort | Token/cost impact | Data source |
|---|---|---|---|---|---|---|---|
| 1 | **"Blocked on you" state from approval entries** | Dock shows which thread/boss session awaits approval, what tool, how long; Enter jumps to the pane; optional desktop/voice notification | PR #18837 (merged, 19.1.0), docs/session.md; #18803, #18934 | HUD + service (session tail) | S | Zero tokens | omp already writes `tool_approval_requested/resolved` |
| 2 | **Stalled/idle thread detector** | Thread badge if run active but no session append / no heartbeat for N min; "waiting on background job" vs "waiting on you" vs "working" | #19901 (judge version), #20272 heartbeats, #20245/#19859 wait semantics, 18.21.0 `agent_pause` | HUD + thread service | S | Zero (deterministic; avoids the judge-model version) | session mtime + custom entries; RPC heartbeat only if RPC used |
| 3 | **Cost view from `stats.db` / archive** | Per-thread/day cost, cache-hit %, tool error counts, approval counts; replace hand-parsing | omp-stats, #20294, Unreleased `archive.stats/calls`, `AssistantMessage.requestId`, cache-warm cost events | HUD + ledger | S–M | Zero | omp already writes usage in sessions + stats.db |
| 4 | **Cache-warm cost line item** | Ledger entries for warm/prewarm spend separate from turn spend; warn if idle warming exceeds budget on boss session | 19.0.0 cacheWarming modes; pi 1.5.1 | ledger + HUD | S | Saves/attributes money; no new calls | cache-warm events / session entries (verify names) |
| 5 | **Version-contract smoke test for omp upgrades** | On `omp` version change run a deterministic probe: plan-mode `propose` detection, `--continue` on live session, overlay precedence, `ps --json`, approval entry shape | §3 break list (`xd→commands`, `ask` form, `agent_end`, `--continue` rule, overlay prepend) | service / CI | M | Zero | none |
| 6 | **Scrollable compact news + thread-first HUD** (user's HUD request) | Use freed space for threads + approvals + cost; news one-line scroll | #1627 shows demand for multi-session overview; Tern tray cards are omp's own answer | HUD | M | Zero | existing feeds |
| 7 | **Route Jocasta guard rules into omp user-level TTSR `command:` probes** (when #20216 merges) | So guard applies inside subagents (`task`) too | #20216, #19893 | service/guard + overlay | M | Zero (deterministic probes) | none |
| 8 | **Deterministic wake-on-event instead of boss polling** | Push "thread X done/blocked" into boss via `write`/message queue rather than boss `wait`-polling | #18485 (token burn from poll loops), bash `notify` option (19.0.0), `agent://peers` | boss + service | M | Saves tokens | session tail events |
| 9 | **Session→branch mapping & thread dashboard** | Show branch/worktree/dirty state per thread from session header | #19924 (open), `omp worktree list --json`? [verify] | HUD | S | Zero | session header (once merged) or git |
| 10 | **Honour `.worktreeinclude` in Jocasta worktree creation** | copy listed ignored files into thread worktree | #19283 | thread | S | Zero | repo file |
| 11 | **MCP read-only auto-allow in guard** | allow MCP tools with `readOnlyHint` | #18219 | guard | S | Fewer prompts, zero tokens | MCP annotations |
| 12 | **Cheap compaction model in overlay** | `compaction.model` for long threads | #20286 (open), pi#11255 | overlay config | S | Cuts summarization cost | n/a — config |
| 13 | **Account-switch reason in ledger** | when #19623 merges | #19623/#19134 | ledger | S | none | RPC event/log |
| 14 | **Phone/remote approvals** | notify + answer approvals remotely | #18803 | boss/service | M–L | none | needs omp API not yet shipped; until then notification-only (idea 1) |

## 6. What to verify before acting (not done: research-only)
- Exact JSON shape of `tool_approval_*` entries and any `agent_pause` marker in a real 19.1.x session file.
- Whether Jocasta's plan-mode detection relies on `exit_plan_mode`/`xd://`; 19.1.0 renamed to `propose` command.
- Whether Jocasta uses `omp ps`, `task.file`, `tools.xdev*`, RPC `ask`, or Gemini CLI provider.
- Merge status of #19934, #20216, #20286, #20294, #19923, #18803, #20272 (all open on 2026-11-23). Predictions in §2 are [INFERENCE].

---

# PiExtensions: ecosystem research (pi/omp extensions, orchestrator HUDs)

Seen 2026-11-23. Sources are linked inline. Download numbers are npm "last month" (window 2026-10-24..2026-11-22, api.npmjs.org) unless noted. Stars come from the GitHub API on 2026-11-23. Anything not verified is marked [INFERENCE]. Nothing was installed, posted or changed.

## 1. Findings (read this first)

1. **The pi ecosystem is large and mostly fills gaps pi left out on purpose.** pi.dev lists **5,350 packages** (https://pi.dev/packages, 2026-11-23). pi itself ships four tools (read/write/edit/bash) and no MCP, sub-agents, plan mode or permissions (https://composio.dev/content/top-pi-extensions, 2026-08-17). The top downloads are exactly those missing pieces: MCP, sub-agents, web, plan review, permissions, context savers.
2. **omp already has most of the head of that list built in**, so the pi top-10 mostly does not apply to us. Verified in the omp README (https://github.com/can1357/oh-my-pi, 2026-11-23): `task` sub-agents with worktree isolation, Agent Hub (`Alt+A`), `todo`, memory tools, MCP, web search, `/review`, fallback chains and round-robin credentials on 429, and `@oh-my-pi/omp-stats` (local usage dashboard). What omp lacks is the cross-session layer (see §3).
3. **The long tail is where the need shows.** Searching npm for `keywords:pi-package`: *dozens* of separate usage, quota, cost, footer and notify packages, with no clear winner. Each is a per-session widget that re-queries the provider. At least five are built to avoid hammering the usage endpoint from many sessions at once (e.g. `@sreetej510/pi-usage` caches in `~/.pi/agent/usage-cache.json` "so multiple concurrent pi sessions don't hammer the provider APIs"). That is the signature of a need that belongs to something above the sessions.
4. **omp's own issue tracker asks for what Jocasta is.** Top open omp issues by reactions (gh API, 2026-11-23): #5742 GUI on top of omp (43), #11600 sticky bottom bar (29), #10232 pinned composer (23), **#8077 cross-session messaging between independent omp sessions (19)**, **#1627 Agent View, multi-session parallel monitoring like `claude agents` (16)**, #8656 many live sessions in one process (13), **#3815 live preview of subagent activity (31, closed or open not checked)**, #436 remote control (23), #2763 scheduled/recurring tasks (8), #2956 toggle approval mode on the fly (18), #2785 concise tool display for low-noise steering (13), #11689 false 429 quota exhaustion (19 reactions, 113 comments).
5. **Orchestrator dashboards converge on the same four jobs**: isolate (worktree), see, unblock (surface "needs you"), review/merge (https://crystl.dev/blog/best-ai-agent-orchestration-tools/, 2026-08-17, a competitor's own framing). The strongest consensus is "an idle agent looks identical to a finished one", so *needs-attention* beats everything else. Cost/quota is second.
6. **The category is volatile.** Vibe Kanban's company shut down April 2026 (28.8k stars, no commits 24 Apr to mid-Sep, resumed; pushed 2026-11-15). Conductor announced it is joining Meta, then said it is not shutting down (crystl article, verified 2026-11-16). Crystal became Nimbalyst (3.1k stars, last push 2026-02-26). Claude Squad (8.9k stars) has open issues "No more updates?" (16 reactions) and "Project Seems Abandoned" (11). Superset's repo now redirects to Paradev. So "build the small deterministic part yourself" is a defensible bet.
7. **Most-asked features in orchestrator issue trackers are boring**: keep the board/familiar UI (vibe-kanban #2687, 33 reactions), self-hosted GitLab (26), worktree cleanup config (#765, 19), worktree location patterns (claude-squad #86, 10), auto session rename (agent-deck #1554). Hygiene, not novelty.

## 2. Ranked pi/omp extensions

Downloads are last-month npm. "Fit" says whether it matters for Jocasta/omp.

### 2a. pi: top by downloads (pi.dev catalog page is sorted by downloads; numbers from npm)

| # | Package | Month dl | Stars | What it adds | Fit for omp/Jocasta |
|---|---|---|---|---|---|
| 1 | [pi-mcp-adapter](https://pi.dev/packages/pi-mcp-adapter) | 2.39M | 1.7k | one ~200-token proxy tool for all MCP servers, lazy loading | omp has MCP; the *lazy proxy* idea is a token saver [INFERENCE: omp mounts rarely used tools as bash commands, README] |
| 2 | [pi-web-access](https://pi.dev/packages/pi-web-access) | 1.31M | 1.8k | web search, URL/PDF/YouTube, 20+ providers; `Ctrl+S` shows what search results entered context | built in to omp |
| 3 | [pi-subagents](https://pi.dev/packages/pi-subagents) (nicobailon) | 968k | 4.2k | delegation, reviewer/oracle/scout roles, background/parallel/chain, live inspector | built in (`task`, Agent Hub) |
| 4 | [context-mode](https://github.com/mksglu/context-mode) | 227k | 34.4k | sandboxes tool output away from context (claims 98% cut), FTS5 session memory; 11 agents | token lever; the output-sandbox part is the interesting one |
| 5 | [@juicesharp/rpiv-ask-user-question](https://pi.dev/packages/@juicesharp/rpiv-ask-user-question) | 175k | 970 (monorepo) | typed questionnaire instead of the model guessing | omp has `ask` [INFERENCE] |
| 6 | [pi-lens](https://pi.dev/packages/pi-lens) | 114k | 537 | LSP/linter/type feedback injected after each edit | omp has LSP built in |
| 7 | [@plannotator/pi-extension](https://pi.dev/packages/@plannotator/pi-extension) | 97k | 14.6k | visual plan review/annotation in browser; code/PR review | **same job as Jocasta's whiteboard review page**; multi-agent (not pi only) |
| 8 | [pi-memory](https://pi.dev/packages/pi-memory) | 90k | n/a | qmd semantic search over daily logs, long-term memory | omp has memory tools |
| 9 | [@tintinweb/pi-subagents](https://github.com/tintinweb/pi-subagents) | 87k | 1.5k | Claude-Code-like sub-agents, live widget, **fleet view**, steering, workflows | fleet view = a HUD pane |
| 10 | [@juicesharp/rpiv-todo](https://pi.dev/packages/@juicesharp/rpiv-todo) | 78k | 970 | todo overlay that survives compaction | omp has `todo` |
| 11 | [@gotgenes/pi-permission-system](https://pi.dev/packages/@gotgenes/pi-permission-system) | 51k | n/a | permission enforcement | = Jocasta guard |
| 12 | [pi-intercom](https://pi.dev/packages/pi-intercom) | 41k | n/a | messaging between pi sessions | = omp #8077 request; Jocasta thread messaging |
| also | [cc-safety-net](https://github.com/kenryu42/cc-safety-net) | 200k | 2.1k | pre-exec guard for destructive git/fs commands, 11 agents | = Jocasta guard |
| also | [@langchain/langsmith-pi-extension](https://pi.dev/packages/@langchain/langsmith-pi-extension), [@braintrust/pi-extension](https://pi.dev/packages/@braintrust/pi-extension), [pi-otel](https://www.npmjs.com/package/pi-otel) | 165k / 43k / 26k | n/a | tracing of sessions, turns, tools to a cloud (OTel for pi-otel) | observability is wanted; Jocasta's ledger is the local version |

Other notable numbers: [@quintinshaw/pi-dynamic-workflows](https://pi.dev/packages/@quintinshaw/pi-dynamic-workflows) (82k dl, 595 stars: 100s of sub-agents, model routing, token/cost accounting, worktree isolation, `/workflows` TUI) and [@casualjim/pi-herdr-agents](https://www.npmjs.com/package/@casualjim/pi-herdr-agents) (3.2k dl: async pi sub-agents inside **herdr**, with optional worktrees) show people already wire pi into herdr. [BlackBeltTechnology/pi-agent-dashboard](https://github.com/BlackBeltTechnology/pi-agent-dashboard) (1.8k dl): one browser tab for many pi sessions, grouped by folder, with branch, live cost, context gauge; **pi only, omp unsupported** (README).

### 2b. omp-specific plugins (npm search `oh-my-pi plugin`, `omp plugin`, `keywords:omp-plugin`, 2026-11-23)

| Package | Month dl | What it adds | Relevance |
|---|---|---|---|
| [@nledford/omp-coord](https://www.npmjs.com/package/@nledford/omp-coord) | 2.8k | **shared sub-agent caps, fair queue, build/test queues, claims on branches/PRs, messages and pause across omp sessions, SQLite event log, footer `coord B · work 5/8`** | the closest thing to Jocasta's service. Proves cross-session caps, queues and claims are wanted. Uses omp's spawn check hook. |
| omp-mobile | 3.2k | serves omp sessions to a phone app over Tailscale | remote control demand (omp #436) |
| pi-zentui | 8.5k | modular TUI components for pi and omp | UI polish demand |
| @sjawhar/pi-legion | 66.6k* | a "Legion pane" plugin beside pi-envoy | *number may be a dependency pull, [INFERENCE]; not investigated |
| omp-discord-presence | 1.9k | Discord Rich Presence, live status | vanity |
| omp-activities | 1.1k | compact tool history and live activity "without extra AI calls" | same preference as ours (deterministic, no model call) |
| omp-harnmem | 0.96k | cross-harness memory, byte-stable prompt injection | cache-friendly memory |
| @nickf93/omp-automode | 0.5k | AI reviewer allows/blocks each risky tool call | = guard, with a model call (costly) |
| @tommasomarchionni/omp-usage | 0.16k | collects LLM usage events | tiny; omp already writes this |
| omp-alamode | 0.6k | bundle: let-me-see, flymode, tokenmunchers, session extras | niche |

The omp ecosystem is about 2 orders of magnitude smaller than pi's (largest omp-only plugin 3k/month vs pi top 2.4M). [INFERENCE: most pi packages also load in omp via its compatibility, not verified here.]

### 2c. Usage / quota / cost / notify cluster (the HUD-relevant long tail)

| Package | Month dl | What |
|---|---|---|
| [@tmustier/pi-usage-extension](https://www.npmjs.com/package/@tmustier/pi-usage-extension) | 17.8k | `/usage` aggregated stats across all sessions (graphs; reconciles sub-agent sessions to avoid double counting) |
| [@lwlee2608/pi-notify](https://www.npmjs.com/package/@lwlee2608/pi-notify) | 9.7k (4.1k last week) | **phone notification when an armed run finishes** |
| [@celljs/ta-notify](https://www.npmjs.com/package/@celljs/ta-notify) | 4.2k (3.8k last week) | tells you when a conversation needs you / failed / answered |
| [@pi-unipi/notify](https://www.npmjs.com/package/@pi-unipi/notify) | 4.6k | native OS, Gotify, Telegram notifications for lifecycle events |
| [pi-notify](https://www.npmjs.com/package/pi-notify) | 2.0k | desktop notifications via OSC 777/99/9 and Windows toast |
| [@cad0p/pi-usage](https://www.npmjs.com/package/@cad0p/pi-usage) | 3.9k | read-only quota for Claude, Codex, multi-account |
| [@sreetej510/pi-usage](https://www.npmjs.com/package/@sreetej510/pi-usage) | 3.7k | Codex/Anthropic rate-limit budgets, shared disk cache, statusline |
| [@deathbeam/pi-usage](https://www.npmjs.com/package/@deathbeam/pi-usage) | 2.8k (1.8k last week) | tokens/cost per provider, model, project |
| [@saifymatteo/pi-claude-usage](https://www.npmjs.com/package/@saifymatteo/pi-claude-usage) | 1.2k | Claude rate-limit windows from provider message diagnostics (no extra call) |
| [@tduyng/pi-cost](https://www.npmjs.com/package/@tduyng/pi-cost) | 1.3k | spend, live rate-limit windows, footer segments |
| [pi-cost-ledger](https://www.npmjs.com/package/pi-cost-ledger) | 0.4k | spend, tokens, cache efficiency across all sessions/projects/models (= Jocasta's ledger) |
| [@luxass/pi-cost-explorer](https://www.npmjs.com/package/@luxass/pi-cost-explorer) | 0.7k | cost by model, prompt, tool, request |
| footers: pi-footer 1.6k, pi-bar 0.7k, @hank-warren/pi-statusline 0.9k, pi-usage-bar, @tzlzl/pi-footer | | model, git, tokens, context pressure |
| worktrees: @narumitw/pi-worktree 1.9k, pi-worktree-preview 1.7k (review isolated worktree changes in the original checkout) | | |

Reference outside pi: **ccusage 1.03M/month** (npm), [Claude-Code-Usage-Monitor](https://github.com/Maciek-roboblog/Claude-Code-Usage-Monitor) 8.8k stars (v4: official statusline `rate_limits`, pace/forecast, provenance labels `official` vs `local_estimate`, local warehouse that survives 30-day cleanup), [agentsview](https://github.com/wesm/agentsview) 6.5k stars (local-first session search, analytics, cost, SQLite; v0.48.0), [coding_agent_session_search](https://github.com/Dicklesworthstone/coding_agent_session_search) 1.2k stars (TUI search over 30+ providers). Cost/usage tooling is the single biggest standalone segment.

## 3. Needs the harness lacks, and where they are best served

| Need (evidence) | Per session or once above? | Why |
|---|---|---|
| **Notify when a session needs me / finished / failed** (4+ notify packages; pi-notify, ta-notify 3.8k/week; cmux "notification rings"; agent-deck, crystl "unblocking") | **Orchestrator** | One notification path for N agents; per-session plugins give N noisy, unaggregated pings. Edge-triggered and deterministic. |
| **Quota / rate-limit windows** (10+ packages; omp #11689 113 comments; pi-usage caches on disk for concurrent sessions) | **Orchestrator** | Quota is per account, not per session. N sessions each polling the provider is waste and 429-prone. Poll once, cache, show one number. |
| **Cost ledger across sessions/projects** (pi-cost-ledger, pi-usage-extension 17.8k, agentsview, ccusage 1M) | **Orchestrator** | Aggregation is the point; per-session widgets cannot sum. Data already on disk. |
| **Cross-session messaging, claims, pause** (omp #8077; pi-intercom; omp-coord) | **Orchestrator** | Requires a shared registry. omp-coord does it as a plugin; Jocasta's service already owns thread identity. |
| **Shared caps and build/test queues** (omp-coord) | **Orchestrator** | Machine-wide resource: N agents running `cargo build` at once. |
| **Plan/diff review UI** (Plannotator 97k dl, 14.6k stars) | Either; **Jocasta already has the whiteboard** | |
| **Guard / permissions** (pi-permission-system 51k, cc-safety-net 200k, automode) | Rules once above, enforcement hook per session | policy centralised, hook per agent |
| **Context/token savers** (context-mode 227k, billion-context 4M, mcp-adapter lazy proxy) | **Per session** | Lives inside the context window; orchestrator cannot help except by reporting context pressure. |
| **Tracing/observability** (LangSmith, Braintrust, pi-otel 26k) | Either; local-first = orchestrator | |
| **Remote/phone control** (omp-mobile, omp #436, Conductor-of-agent-deck via Telegram/Slack) | **Orchestrator** | One bridge, not N. |
| **Live subagent/session overview** (omp #1627 Agent View, #3815, tintinweb fleet view, blackbelt dashboard) | **Orchestrator** | By definition cross-session. |
| **Scheduled/recurring tasks** (omp #2763) | **Orchestrator** | needs a daemon that outlives sessions |

## 4. What orchestrator dashboards show

| Tool (stars) | Shows | Notes |
|---|---|---|
| [Vibe Kanban](https://github.com/BloopAI/vibe-kanban) (28.8k) | kanban cards per task, agent status, diff review, PR | board shape; users fought to keep it when UI changed (#2687, 33 reactions) |
| [Claude Squad](https://github.com/smtg-ai/claude-squad) (8.9k) | tmux list of sessions, live pane preview, diff tab, review before push | most-reacted issues are tmux pane capture errors and "abandoned" |
| [agent-deck](https://github.com/asheshgoplani/agent-deck) (1.3k, active today) | **working / waiting / done** per session, groups, fork, search, cost dashboard + `agent-deck usage` quotas, conductor session that supervises others and escalates to Telegram/Slack/Discord, **watchers** (GitHub, gmail, ntfy "doorbells"); supports **omp and pi** with hook-driven status | closest to Jocasta's shape; TUI (Bubble Tea) |
| [cmux](https://github.com/manaflow-ai/cmux) (28.9k) | sidebar per workspace: git branch, **linked PR status/number**, cwd, listening ports, latest notification text; blue ring on pane that needs attention; notification panel | README names omp in its SSH example |
| [emdash](https://github.com/generalaction/emdash) (6.3k), [Paradev/Superset](https://github.com/paradevai/paradev), Conductor | workspace list, Changes (diff), PR status, tests, browser preview, mobile/web check-in; agents can drive the app via CLI/MCP | |
| [BlackBelt pi dashboard](https://github.com/BlackBeltTechnology/pi-agent-dashboard) | sessions by folder, branch, spec context, live cost, context gauge, phone | pi only |
| [Claude-Code-Usage-Monitor](https://github.com/Maciek-roboblog/Claude-Code-Usage-Monitor) (8.8k) | burn rate, window progress, pace/forecast, time to limit, confidence label | the quota HUD people like |
| [workmux](https://github.com/raine/workmux) (3.1k) | worktrees + tmux windows, status in tmux bar | minimal |

**Most liked / most asked [by evidence above]:**
1. Needs-attention surfacing (every comparison puts it first; cmux, crystl, agent-deck, 4 notify packages).
2. Cost + quota with reset time and forecast (ccusage 1M/month, usage monitor 8.8k stars, 10+ pi packages, agentsview 6.5k).
3. Per-session live activity and "what is it doing now" (omp #3815 31 reactions, #1627 16).
4. Diff and PR/CI state in the list row (cmux sidebar, Conductor, emdash).
5. Familiar board shape / stable UI (vibe-kanban).
6. Remote/phone check-in and notifications.
Not requested much in the data: timelines/gantt, agent "personality", token-by-tool flame graphs (cost-explorer has only 0.7k dl).

## 5. HUD ideas for a narrow right panel (~35%, roughly 40-60 columns)

Layout principle: dense one-line rows, color = state, everything derived from files omp already writes or git/gh, no model calls. Top-down by decision value:

```
 ATTENTION  2                      <- always row 1; collapses to "all quiet"
 ! T-fix-auth   asks: apply plan?  4m
 ! T-docs       blocked: guard     1m
 QUOTA  Claude 5h ▓▓▓▓▓▓░░ 71% ⟳1h12m
        week      ▓▓░░░░░░ 28%  ~lasts
 TODAY  $4.20  (7d avg $6.10)  
 THREADS  (row = state, branch, tests, diff, age)
 ▶ fix-auth   run  ✓tests +84 −12  6m
 ⏸ docs       wait  ·            12m
 ✔ lint-pass  rev  PR#41 ci✓     1h
 QUEUE  build 1/2 · test 0/1
 NEWS (scroll, 1 line each)  ↑↓
 ...
```

| # | Idea | Why worth the space | Data source (no new model calls) |
|---|---|---|---|
| 1 | **Attention strip** at the top: threads awaiting Allow, plan approval, a question, a guard block, a failed check, a finished thread. Count + one line each, jump key. | #1 asked feature; agent-deck/cmux/crystl all centre on it | thread state, guard events, omp events |
| 2 | **Quota meters with reset countdown and pace** (5h and weekly window; "will last / will hit limit at HH:MM"). Poll the provider once, cache, label `official` vs `estimate` (usage monitor v4 convention). | Quota is account-wide; per-session widgets duplicate polling; omp #11689 shows the pain | provider usage endpoints (HTTP, deterministic, no model), or the 429 and rate-limit data omp stores [INFERENCE: not verified which fields omp persists] |
| 3 | **Today's spend**, with 7-day trend sparkline and top thread by cost. | ledger exists; ccusage/agentsview demand | Jocasta ledger (already) |
| 4 | **Thread rows with git facts**: state glyph, branch, +/- lines, last check result, PR# and CI, age since last activity. | cmux's sidebar proves branch/PR/ports are what people want per row | `git diff --shortstat`, check results, `gh pr` (cache) |
| 5 | **Stuck / idle detector**: row turns amber after N minutes with no tool call or token; shows last tool name. | "idle looks like finished" problem | session file mtime and last event |
| 6 | **Live activity line per running thread** (last tool + file, 1 line, no model). | omp #3815 (31 reactions) live preview; omp-activities does this "without extra AI calls" | omp session events |
| 7 | **Machine queue/cap gauge**: build/test slots in use, sub-agent cap, agents running now. | omp-coord shows this footer is useful | Jocasta service state |
| 8 | **Context pressure per thread** (% of window, compaction soon). | pi footers (pi-bar, custom-footer) all show it | session usage fields |
| 9 | **Scrollable compact news**: 1 line per item, fixed height (5-6 rows), `j/k` scroll, `enter` opens in the boss; collapses when attention strip is non-empty. | already requested | feed |
| 10 | **Day strip**: next calendar event within 2h and the one open commitment. Only when something is due. | space only when non-empty | calendar |
| 11 | **Weekly drift / lessons nudge**: one line "review due" only on its day. | | existing |
| 12 | **Mode switch (`Tab`)** between "now" (attention, quota, threads) and "history" (spend by day, thread timeline, cache efficiency). Keeps the narrow panel from becoming a wall. | pi-cost-explorer's low adoption suggests deep drill-down is niche; put it behind a key | ledger |

Space rules: rows ≤ 1 line; hide empty sections; show numbers, not charts, except one quota bar and one sparkline; color only for state. [INFERENCE: layout widths are my estimate, not tested in the dock.]

## 6. Ranked feature ideas for Jocasta

Effort S/M/L. "Existing data" = already written by omp/Jocasta; "network" = deterministic HTTP, no model; "model" = new model call.

| Rank | Idea | Evidence | Lives in | Effort | Token/cost | Data |
|---|---|---|---|---|---|---|
| 1 | **Attention strip + one notification path** (OS toast / phone push, edge-triggered, de-duplicated across threads; rule: notify only on "needs you", "failed", "finished" thread) | 4 notify packages incl. 9.7k/month phone notify; ta-notify 3.8k/week; cmux rings; crystl framing | HUD + service | S-M | zero (saves the user's polling) | existing |
| 2 | **Account-level quota meter with reset time and pace**, one cached poller, provenance label | 10+ pi quota packages; usage monitor 8.8k stars; omp #11689 | HUD + service | M | zero tokens; one HTTP call per N min | network + existing 429 data |
| 3 | **Thread rows with branch, +/- lines, last check, PR/CI** | cmux sidebar; Conductor, emdash | HUD + thread | S | zero | git + existing check results |
| 4 | **Stuck-thread detector** (no activity N min, waiting on guard, repeated failing check) → attention strip | idle-vs-finished problem; omp #3815 | service + HUD | S | zero (heuristic); optional model summary only on demand | existing |
| 5 | **Spend panel**: today, 7-day trend, top thread, cache hit ratio, budget line with soft cap that warns the boss | ccusage 1M dl; pi-cost-ledger; agentsview 6.5k stars | HUD + ledger | S | zero; cap saves tokens | existing ledger |
| 6 | **Live one-line activity per running thread** | omp #3815 (31); omp-activities; tintinweb fleet view | HUD | S-M | zero | existing session events |
| 7 | **Machine-wide caps and build/test queues** (cargo/bun jobs across threads) | omp-coord 2.8k dl; measured need when N threads build | service + guard | M | zero; avoids wasted retries | new, via omp spawn/guard hooks |
| 8 | **Thread-to-thread messages and claims** (thread A tells B it owns a path/branch) | omp #8077 (19), pi-intercom 41k | boss + service | M | small (messages are context) | new |
| 9 | **Scrollable, collapsed news** (fixed height, 1 line/item, hide when attention non-empty) | user request | HUD | S | zero | existing feed |
| 10 | **Phone/remote check-in bridge** (read-only status + approve/deny via Telegram/ntfy, like agent-deck conductor and omp-mobile) | omp #436 (23), agent-deck conductor, omp-mobile 3.2k | service | M | zero | existing + bridge |
| 11 | **Scheduled/recurring thread tasks** ("every morning, run drift check") | omp #2763 | service | M | model cost per run; keep deterministic trigger | new |
| 12 | **Doorbell watchers** (GitHub CI failure, PR review comment wakes the boss) | agent-deck watchers | service + boss | M | one model turn per real event only | network |
| 13 | **Worktree hygiene**: stale-worktree list and one-key cleanup | vibe-kanban #765 (19), claude-squad #86 | thread | S | zero | git |
| 14 | **Context-pressure column** | pi footers | HUD | S | zero; steers compaction | existing usage fields |
| 15 | **Plan/diff review parity check vs Plannotator** (14.6k stars): features worth borrowing for the whiteboard (annotate agent messages, review PRs) | Plannotator | whiteboard | M | zero for UI; any summary is a model call | existing |
| 16 | **Local cross-agent session search/analytics** (we already have `history_search`; agentsview shows 6.5k stars for the broader version) | agentsview, cass | boss | S (exists) | zero | existing |

Deterministic first: ranks 1-6, 9, 13, 14 need no new model calls at all. Model calls appear only in 11 (per run) and 12 (per real event).

## 7. Caveats

- Package download counts are monthly npm and include CI/bot pulls; stars of monorepo packages are the monorepo's (rpiv, supi).
- pi-mono moved to `earendil-works/pi`; stars quoted for `badlogic/pi-mono` (123.5k) and `can1357/oh-my-pi` (39.7k) on 2026-11-23.
- Did not read Discord (no login). Issue reactions are used as demand evidence instead. The awesome list `qualisero/awesome-pi-agent` is retired (its README says so); `awesome-pi.site` exists but was not parsed.
- Superset/ccusage GitHub stars were not fetched (API rate limit); ccusage's npm downloads are above.
- Not verified: which rate-limit/quota fields omp persists to session files (needed for idea 2 without network); whether #3815 is open.

---

# OmpDiscord: omp community research (seen 2026-11-23)

## 1. Access result (first)

**Discord message history is NOT readable without logging in.** Facts:

| Check | Result |
|---|---|
| Invite in README/CONTRIBUTING | `https://discord.gg/4NMW9cdXZa` (README badge, line ~18 and ~697); vanity alias `discord.com/invite/ompsh` (issue #11906: https://github.com/can1357/oh-my-pi/issues/11906). The `4NMW9cdXZa` code reportedly fails to accept for some users; `ompsh` worked. |
| Public invite API `discord.com/api/v9/invites/4NMW9cdXZa?with_counts=true` | Metadata only: guild "omp", id `1465833614603325562`, "for all your agentic omp needs", ~6,962 members / ~1,922 online, est. 2026-01-27, DISCOVERABLE+COMMUNITY, default channel `💬-general`. No messages. |
| Discovery page https://discord.com/servers/omp-1465833614603325562 | Loads without login; shows only name, counts, description. No message content. |
| `https://discord.com/channels/1465833614603325562/1465833615509426342` in a fresh headless tab | Redirected to `discord.com/login?redirect_to=...` ("Welcome back! ... Email or Phone Number / Password / Log in with QR Code"). Login wall. Stopped there; tab closed, nothing entered. |
| Widget `…/guilds/…/widget.json` | HTTP 403 (widget disabled). |
| Answer Overflow (`answeroverflow.com/c/<guildId>`) | HTTP 429 via fetch; browser tab hit "Vercel Security Checkpoint". Could not confirm whether the server is indexed; no search hit pointed to an Answer Overflow page for omp. [INFERENCE] likely not indexed (forum-channel indexer; unverified). |
| Linen / GitHub Discussions mirror | None found. |
| Notable | CONTRIBUTING.md says upstream changes must be proposed in Discord first ("take any proposed upstream change to Discord before implementation, as CONTRIBUTING requires" quoted by a contributor in #8077), so design talk lives there and is invisible to the web. |

Fallback used (all public, no login): GitHub Discussions (436 open, https://github.com/can1357/oh-my-pi/discussions), issue #8077, PR #18622, Hacker News. Search engines were mostly rate-limited; Reddit/X queries returned nothing usable (X: one promo tweet only, https://x.com/noone_0x/status/2058891847800402366, no substance).

## 2. Findings from public places (last ~2 months unless noted)

### Cross-session orchestration is the loudest unmet need
- Issue #8077 "cross-session messaging between independent OMP main sessions (Claude Code parity)" (opened 2026-08-09, 11 comments through 2026-11-23): https://github.com/can1357/oh-my-pi/issues/8077
  - Today `task`+`hub`/IRC is process-local; two terminals cannot message each other. Maintainer bot recommends a manual opt-in MVP; core PR #7255 / #14263 still open. [INFERENCE] native support is not imminent.
  - 2026-11-23 ValTM: "in multi-agent sessions it's so much faster with claude than omp having the models read-write to a file and wake up the other sessions".
  - 2026-10-12 mg-mg-mg (5 long-lived sessions on one repo): relay through an external agent cost "about 55 minutes per step"; lessons: liveness needs process start time + pid (pid reuse), "sender fields in a shared mailbox are claims, not authentication" (#12185).
  - 2026-11-03 qiangu2020: writing `agent://<other-session-uuid>` gives "is not running" although the owner is alive (misleading diagnostic; registry is process-local).
  - 2026-11-12 Taha-Mahmoodi: "**Turn ended does not mean task done**: the current peer API does not report background-work quiescence"; wants explicit DONE/BLOCKED/WAITING; peer IDs rotated after restart so master role had to be re-claimed ("stable per-session addresses").
- Ecosystem tools that fill the gap (all out-of-tree plugins/apps):
  - **omp-peers** (PIIIX-org, 2026-11-17): master session, peer msg/status/tail/steer, standing rules ("max 2 subagents per session"), **decision inbox** (sessions queue questions for you), shared task board, pause/resume wake gate, task dependencies, **named locks/semaphores** ("only one browser check or heavy build at a time"), watchdog for stuck sessions, per-session usage/cost, read-only terminal dashboard, audit log, Discord bridge. Runs 9 sessions/4 projects. https://github.com/can1357/oh-my-pi/discussions/19561
  - **OMP Studio** (the-snowpear, 2026-11-19): desktop workbench on a typed RPC bridge (`omp --mode studio-host`), motivation: "once I started running 2–3 sessions at once (waiting on a plan approval in one, watching a subagent finish in another...) bouncing between tmux panes got tedious". Features: Agent Hub orbit view of handoffs with per-agent tokens/spend, native `omp stats` (cache hit rate, costliest calls), per-turn workspace snapshots ("Time Travel"), git worktrees view, model benchmarking. https://github.com/can1357/oh-my-pi/discussions/19811
  - **Pi on the Side!** (Jumpstile, 2026-11-14, Windows): lets Claude/ChatGPT queue tasks into OMP sessions per project, "built-in watcher notices when a session goes idle", wake-up notifies you; receipts "say what happened, not whether the work was good"; v1.0.2 fix: warn at once when target session isn't running. https://github.com/can1357/oh-my-pi/discussions/19284
  - **omp-council** (2026-10-16): multi-model debate, "the ledger (code), not a model, decides consensus", live board above editor with spend, USD/token/round budgets. https://github.com/can1357/oh-my-pi/discussions/15939
  - agent-collective / omp-peers (nikkoxgonzales): 4-hop relay cap, 400 ms burst coalescing, **20 wakes/peer/hour budget** (loop/cost guards), per #8077 comment 2026-09-08. Concord MCP: claim files before editing, handoffs stay with sender until accepted (ack).
  - omp-session-roles (per-session model role overrides, https://github.com/can1357/oh-my-pi/discussions/16496), omp-settings-web (web UI for 529 settings, https://github.com/can1357/oh-my-pi/discussions/15235), Helicon imports omp usage (https://github.com/can1357/oh-my-pi/discussions/15325), AgentManager catalog entry.

### Status line / HUD asks
- Discussion #5830 (open since 2026-07-17; 👍 comments 2026-08-17, 08-22, 11-17, 11-23): show **absolute context tokens** `13.6% (136K/1M)`; "the absolute value is more useful to me"; "what really affects the outcome is the absolute value". https://github.com/can1357/oh-my-pi/discussions/5830
- Discussion #16802 "add task completion time" (2026-10-22, Ideas). https://github.com/can1357/oh-my-pi/discussions/16802
- Status line example from #5830: `⌥ π > ● Opus 4.8 · high > ~/.omp > ▯ 13.6% (136K/1M) ↺ > $5.29` (omp already shows cost per session).

### Costs, quotas, providers
- 2026-11-09 PR #18622 (merged): a KDL `disabled "<reason>"` auth rule pauses Google Antigravity "because sign-ins from omp were getting people's Google accounts restricted" (HN: https://news.ycombinator.com/item?id=50431613). Discussions #19809 "Antigravity alternatives", #19230 (429/fingerprinting). Takeaway: subscription-account bans are a live user fear; quota/rate-limit state matters.
- #9950 smallest models for omp (unanswered), #18391 token efficiency vs Pi, #17522 Devin subscription. OpenRouter route identity/limits ask (#15936).
- HN (2026-08-14): omp `/shake` turns tool-call bloat into artifact refs; `/btw` side questions without bloating context; users like it (https://news.ycombinator.com/item?id=49294360).

### Durability / unattended
- #16400 (2026-10-19, 15 👍): adopt Pi 1.0's pi-durable ideas (commit-before-display, checkpoint every step, crash-loop parking with `retryTask`, idempotent `requestId`). Cites omp issues #6947 (durable workflow DAGs), #14386, #14898 (no-input continuation after restart), #11564, #10311: "unattended headless runs, background fleets, keep-alive subagents are exactly where lost work hurts most". https://github.com/can1357/oh-my-pi/discussions/16400

### Memory
- #3816: project-scoped memory exists (`memory.backend: local`) but artifacts live under the agent dir, not the repo; no setting to redirect; `.omp/skills/<name>/SKILL.md` is discovered (answer 2026-11-17, SheetHappiness). https://github.com/can1357/oh-my-pi/discussions/3816

### Windows
- #8077 (nikkoxgonzales): `omp plugin install <local-path>` hits EPERM on Windows (symlink w/o junction type); marketplace path works. Pi on the Side is Windows-only tested. Otherwise Windows chatter is thin in what I could read.

### Not found (do not assume)
- Little public discussion of worktree-per-thread flows, plan-mode pain, or notifications beyond the above; those are likely in Discord (unreadable). Reddit/X yielded nothing verifiable.

## 3. Ranked feature ideas for Jocasta

Jocasta already has: boss, threads in worktrees, `today` dock, cost ledger, guard, `history_search`, whiteboard. Ideas below mostly confirm that Jocasta's design lines up with the community's unmet needs.

| # | Idea | Evidence | Lives in | Effort | Cost / data |
|---|---|---|---|---|---|
| 1 | **Decision inbox in the dock**: one list of every thread/session question or plan-approval waiting on Zhivko, with age and one-key jump | omp-peers "decision inbox"; Studio "waiting on a plan approval in one" | HUD + service | S-M | Deterministic; uses thread state/events omp already emits. Zero model calls |
| 2 | **True "done" state**: thread row distinguishes turn-ended vs background work quiescent vs explicit DONE/BLOCKED/WAITING yield | Taha-Mahmoodi 2026-11-12 "Turn ended does not mean task done" | thread + service + HUD | M | Deterministic from session events + yield result; no model calls |
| 3 | **Per-thread cost/token strip with absolute context** (`136K/1M`, cache hit rate, $ per thread, burn rate), sorted by spend | #5830 (4 supporters), Studio `omp stats` panel, omp-peers per-session cost | HUD | S | Reads session files/ledger already written; zero model calls |
| 4 | **Stuck/idle watchdog + wake budget**: flag a thread silent N min or looping; cap wakes (e.g. 20/peer/hour), 4-hop relay cap | omp-peers watchdog; nikkoxgonzales limits | service + HUD | S-M | Deterministic timers over session mtime/events; protects spend |
| 5 | **Named locks/semaphores for shared resources** (one browser check, one heavy build, one dev server port across threads) | omp-peers locks | service (guard-adjacent) | M | Deterministic; no model cost; avoids duplicate/colliding work |
| 6 | **Quota/rate-limit and account-risk panel**: provider 429s, usage windows, paused providers (Antigravity KDL `disabled` rule) | #18622, #19230, Antigravity bans | HUD + ledger | S-M | Uses omp usage/auth data it already holds; deterministic |
| 7 | **Standing rules registry** applied to all threads ("max 2 subagents", budgets) instead of repeating prompts | omp-peers | boss/service | S | Fewer prompt tokens; deterministic injection |
| 8 | **Task dependencies between threads** ("when A done, start B"), shared board visible in dock | omp-peers task deps/board; #8077 frontend/backend contract case | boss + service | M | Deterministic scheduler; no model calls beyond the threads |
| 9 | **Handoff with ack**: thread A hands contract change + commit + files + tests to B; stays pending until B accepts | Concord MCP | boss/thread | M | Small structured message; cheap; needs omp message delivery (boss steer today) |
| 10 | **Idle/finished notifications outside the terminal** (toast/phone/Discord-style bridge), de-duplicated by event id | Pi on the Side watcher; omp-peers launchd watcher + dedupe | service | S-M | Deterministic; no model calls |
| 11 | **Time-travel snapshots per turn** (revert code, chat, or both) | OMP Studio | thread | L | Disk only; git worktree makes it cheap; no model calls |
| 12 | **Crash recovery/parking**: persist accepted pending input, resume from last checkpoint, park crash loops | #16400 and cited issues | service | M-L | Deterministic; partly depends on upstream omp durability |
| 13 | **Multi-model council as a gated review step** with ledger-decided consensus and USD budget | omp-council | thread review | M | New model calls (3-4x cost); only for high-risk merges, opt-in |

Evidence gaps: Discord content (design discussions, Windows, worktrees, notifications) could not be read; the only way is a logged-in member (user's decision, not done).
