# 0348 Phase 6 as-built, browser and subagent placement

- Status: decided
- Kind: product
- Phase/slice: 6/6.1–6.9
- Date: 2026-10-08

## Question
Are phase 6's built choices accepted; which browser does the browser tool use; and may subagents run in plan mode and project threads?

## Facts
Verified 2026-10-08.
- Chrome is installed at `C:\Program Files\Google\Chrome\Application\chrome.exe`, and Edge too; both speak CDP.
- omp's plan mode allows subagents: its plan prompt (`packages/coding-agent/src/prompts/system/plan-mode-active.md`) has the agent explore with parallel `scout` subagents and optionally a critique subagent, whose own prompt (`plan-mode-subagent.md`) says to explore and report while the main agent writes the plan. Claude Code allows its read-only Explore and Plan subagents in plan mode.
- As built, `subagent`, `wait` and `send` were denied in plan mode and in chat-only project threads.
- The climb loop was explained at his request.

## Options
| Option | Pros | Cons |
|---|---|---|
| Read-only subagents allowed in plan mode and project threads; `worker` only after approval or in other tabs | Planning can explore in parallel, as in omp; nothing writes behind the plan | One more guard rule |
| All subagents everywhere | Simplest | A worker can change files before the plan is approved |
| Keep blocked | Safest | No parallel exploration while planning |

Browser: Chrome first, Edge as fallback | Edge first.

## Recommendation
Read-only subagents allowed; Chrome first.

## Choice
- The browser tool uses Chrome first and Edge as the fallback, in its own profile per thread.
- Read-only subagents (scout, reviewer, and his own agents whose tools are read-only) run in plan mode and project threads; a child spawned there inherits the same limits and is told to explore and report. `worker` runs only after the plan is approved or in other tabs.
- Accepted as built: plan mode as a flag on yolo or normal, `Plan:` with numbered steps, the "Approve this plan?" card with "Approve and run" and "Keep planning", `/plan`, a headless run stopping at its plan with exit 3; the `--no-plan` flag for unattended runs; the `job` tool (`job-<n>` ids, output kept in memory, jobs end with the session); LSP diagnostics in edit results within 3 seconds, worst first, at most 20, the `diagnostics` tool, the five servers and the install offer once per session, never headless; `python(code, timeout)` with one process per thread and a 120-second default; the `browser` tool's five actions on one 1280×800 page; advisor notes as `<advisory severity=…>`, interrupting only while running, then 3 quiet turns, toggled only by `/advisor`; stream rules firing once per session on text and tool-call arguments; model roles from `roles`, the boss on `jarvis/default` at `low`, memory work on `jarvis/cheap`, Graphiti on the newest Sonnet and Haiku unless `roles.graphMain` and `roles.graphSmall` are set; the `subagent`, `wait` and `send` tools, children as `jarvis` in RPC mode with no session file, their approvals in his tab; the climb loop (`bun evals/climb.ts`, kept changes as patches under `<data>/evals/climbs/`, never applied automatically); `web_fetch` (5 MiB, 30 seconds, 10 redirects, each checked by the guard) and read-only `conflict://`, `pr://` and `issue://`.

## Reason
"maybe google can be default browser for browser interactions or?"; "explain climb and also how does omp or other agents cofingure subagents i think they are okay in plan mode or even default?"; then chosen.

## Consequences
- The in-flight cap's mechanics (shared lock files under `<data>/inflight/`) were left unticked and come back with the next as-built list.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
