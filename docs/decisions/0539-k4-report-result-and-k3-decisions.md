# 0539 K4 and K3: threads hand in a structured report through report_result, and its decisions go to Graphiti and lesson offers

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V10, V11
- Date: 2026-11-08

## Question
The harness-OS layer (0532) wants a thread's outcome as data rather than prose (K4), and its settled choices kept so later threads don't reopen them (K3). How does a thread hand in its report, what does it carry, what if it's missing, and where do its decisions go?

## Facts
Verified 2026-11-08.
- v1 had a `report` tool (0247, 0359: status, summary of at most 200 characters, commits, follow-ups), retired when threads became omp (0386, 0388); a thread's end is read from herdr and its session file, then Jocasta's review runs (hunk, the approved plan and rules, the project's check).
- omp's `yield` gives structured results to subagents only; opencode's and Claude Code's JSON-schema outputs need headless runs.
- Threads see one MCP tool, `graph_search` (`src/mcp/server.ts` `mcpTools`).
- Lessons are a per-project skill in `<data>/projects/<pid>/lessons/`, saved only after his yes and read before planning (0459); Graphiti gets handoff and compaction summaries (0400).

## Options
| Question | Options (pros / cons) |
|---|---|
| Channel | `report_result` tool (fixed rule) · Parse the last message (no tool / fuzzy, against principle 2) |
| Contents | Outcome, summary, files, tests, decisions, questions, follow-ups (feeds K3 and the review) · 0247's shape (less for K3 and the review) |
| Missing | One steer, then the review goes on (one short turn) · The review goes on (reports often missing) |
| Decisions | Graphiti and lesson offers (no new store) · A decisions file per project (a third memory) · Drop K3 (threads may reopen them) |

## Recommendation
`report_result`; the full contents; one steer; Graphiti and lesson offers.

## Choice
- Threads get a second MCP tool, `report_result`, which Jocasta validates against one schema and stores with the thread:
  ```
  report_result({
    outcome: "done" | "blocked" | "failed",
    summary: "…",                                   // ≤ 200 characters
    files: ["…"],
    tests: [{ command: "…", passed: true }],
    decisions: [{ question: "…", choice: "…", why: "…" }],
    questions: [],                                  // open, for him
    followUps: ["…"]
  })
  ```
- The boss's `thread_status`, the review and the task file (K2) read the report.
- A thread whose turn ends done without a report gets one steer asking for it; if the next turn ends without it too, the review runs and says "no report".
- The report's decisions go to Graphiti as an episode, found by `graph_search`; the boss offers the lasting ones to him as lessons (the existing Allow/Deny). No new store.

## Reason
"report_result tool"; "The preview"; "One steer, then go on"; "Graphiti + lesson offers".

## Consequences
- A tested pass in the review stays the proof (0453): a report's `passed: true` is the thread's claim, not the proof.
- The review prompt gains the report.
- `report_result` claims a tool name in plan appendix I.
