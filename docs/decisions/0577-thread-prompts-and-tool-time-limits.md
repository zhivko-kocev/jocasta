# 0577 Thread prompts and the tools' time limits

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V8, V10, V11
- Date: 2026-11-09

## Question
The review found that long tool calls outlive omp's MCP timeouts, and that the threads' and A6's prompts carry the wrong title, Ponytail's license header, and nothing about finishing. What are the limits, and what do the prompts say?

## Facts
Verified 2026-11-09 by the review (0573), reports SET (SET-1), DUP (DUP-4), PRM and SVC (SVC-12).
- omp 18.18.2's MCP default is 30 s (`mcp/timeout.ts:3`), and nothing resets the timer. Setup gives only the boss a timeout, `BOSS_MCP_TIMEOUT_MS = 300_000` (`src/setup/setup.ts:346`); the threads' entry and A6's `.mcp.json` have none, and `jocasta mcp`'s fetch keeps Bun's 5-minute idle cut.
- The service's own bounds: `CHECK_TIMEOUT_MS` and `WEEKLY_REVIEW_MS` 30 min, `COMMIT_TIMEOUT_MS` 10 min, `HANDOFF_HOLD_MS` 5 min; `check` and `big_picture_review` wait for the whole run. Two reviews of one project kill each other's processes (SVC-12).
- A6's prompt reads `# Your task: the benchmark task` (`taskPrompt({ title: "the benchmark task", … })`, bench.ts:139-146).
- Ponytail's license and meta comment are 1,457 of `config/ponytail.md`'s 4,440 B and ride verbatim in every appended system prompt; f-ponytail-if's header differs.
- `prompt.md` says nothing about `check`, `report_result` or committing; `COMMIT_LINE` rides only in the first message.
- `report_result` and `check` are mounted in every omp session on his machine (migration 1's global entry) and only fail outside a thread.

## Options
| Item | Options (pros / cons) |
|---|---|
| MCP limits (SET-1 A, DUP-4 A, PRM-2 a) | Timeouts that cover the service's bounds, no client idle cut (tools stay synchronous / a hung service holds a turn up to 31 min) · Long tools answer early (works under any timeout / changes tool contracts, more turns) · Keep and document (no code / failures stay) |
| PRM-3 big_picture_review | (a) start and reply at once, result as an `[event]` (boss stays free / one more path) · (b) a 31-minute boss timeout (no code / boss blocked) |
| PRM-5 A6 title | (a) what New thread gives (the product's shape) · (b) a fixed neutral title |
| PRM-6 Ponytail header | (a) strip a leading `<!-- … -->` block wherever Jocasta appends it · (b) move the license to its own file (changes the shipped layout) · (c) keep |
| PRM-7 finishing | (a) a `## When you finish` section in `prompt.md` (survives compaction) · (b) fold into `COMMIT_LINE` (lost after compaction) · (c) keep the steer (a turn per task) |
| PRM-9 tool scope | (a) list `report_result` and `check` only with `JOCASTA_THREAD` set · (b) a per-thread MCP entry (worktrees are his repos) |

## Recommendation
A / (a) for every item.

## Choice
- MCP limits: setup writes `"timeout": 1860000` on the global `jocasta` server (migration 1; the 30-minute check plus 1 minute) and `"timeout": 2100000` on the boss's (was 300000); A6's layer `.mcp.json` gets 1860000; `jocasta mcp`'s fetch to the service has no idle cut (`timeout: false`).
- PRM-3: `big_picture_review` starts the review and replies at once: "Started acme-api's big-picture review; its result comes as an [event]."; the result arrives as one `[event]` line; one review per project at a time (which also fixes SVC-12). The boss manual's line 49 becomes "a read-only review of what landed in the project in the past week; it starts at once and its result arrives minutes later as one `[event]` line."
- PRM-5: A6's prompt title is what New thread gives (`briefTitle(trial.prompt)`, with the checkout's folder name as the project): `# Your task: Fix the flaky retry test (acme-api)`.
- PRM-6: a leading `<!-- … -->` block is stripped wherever Jocasta appends Ponytail (threads, A6, project panes, the weekly review, the runner's A1–A5); the shipped file keeps its license.
- PRM-7: a `## When you finish` section in the thread's `prompt.md`, after `## Done when`:
  - "Run the task's checks with the `check` tool (the project's check and every Done-when command, cached by tree); use bash only for a single test file while you iterate."
  - "Commit the approved work on this branch with a short message; the review starts from your commit."
  - "Then call `report_result` once."
  `COMMIT_LINE` leaves the brief (it moves there).
- PRM-9: `jocasta mcp` lists `report_result` and `check` only when `JOCASTA_THREAD` is set (thread panes; A6's layer sets the same); other omp sessions see `graph_search` only.

## Reason
"A" for the time limits; "(a)" for PRM-3, PRM-5, PRM-6, PRM-7 and PRM-9.

## Consequences
- Amends 0484 (the boss's MCP timeout), 0539 and 0540 (`report_result`'s scope and the finish steps in `prompt.md`), 0542 (`check`'s scope and limit), 0546, 0470 (the weekly review's delivery as an event), 0427/0469 (Ponytail's appended text) and 0543/0567 (A6's prompt).
- Setup shows the new timeouts as changes to migrations 1 and 2 on an existing install; the plan's appendix I records the three MCP timeouts (1860000, 2100000, no client idle cut).
- A6's prompt changes before the weekend run, so every A6-family row measures the new prompt.
