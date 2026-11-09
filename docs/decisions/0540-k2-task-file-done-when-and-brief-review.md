# 0540 K2: a task file per thread, its brief and done-when given at launch, and done_when checked in the review

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V9, V10
- Date: 2026-11-08

## Question
The harness-OS layer (0532) keeps a thread's durable state outside its transcript (K2). Where is it, in which format, does the thread see it, and do 0513's "reviewer reads the brief" (B1) and "`done_when` checks" (B2) come with it?

## Facts
Verified 2026-11-08.
- 0513 (after v1) chose a task file per thread in the data folder (brief, approved plan, each review's proof, outcome), tasks as code in the repo only in his own new projects, and not "`/handoff` starting from the task file"; it kept B1 and B2 for after v1 (B2 reopens 0248).
- The brief isn't stored after it's sent, and the reviewer never sees it (`src/frontier/server/hunk.ts` `reviewPrompt`).
- The layer is judged by cost per successful task; the product's success is the review's pass, with no stated acceptance.
- omp's `--append-system-prompt <file>` adds text to the system prompt at launch: compaction keeps it, the prompt cache holds it, and a change takes effect at the next launch.
- Backlog.md (MIT, TypeScript/Bun, v1.55.3) keeps agent-edited tasks in a `backlog/` folder in the repo, with a CLI, a board and an MCP server; its sections have no place for review proofs, lineage or spend.

## Options
| Question | Options (pros / cons) |
|---|---|
| Format | Our own (only what Jocasta uses) · Backlog.md's (its tools could read it later / fields we don't fill; proofs end up as free text) |
| Thread sees it | Brief and done-when at launch (survive compaction, cached) · Path only (may be forgotten after compaction) · Not at all (no help against replanning) |
| B1 and B2 | Both now (a definition of success) · B1 only · Neither (0513 stands) |

## Recommendation
Our own format; brief and done-when at launch; both now.

## Choice
- **File:** `<data>/projects/<pid>/threads/<tid>/task.md`, written by Jocasta only:
  ```markdown
  # fix-login (acme)

  - Thread: t-7f3a · session 2026-11-08T21-14_… · parent: — · base a1b2c3d
  - State: done and reviewed · spend $1.84

  ## Brief
  Login retries hammer the API; back off and fix the flaky auth test.

  ## Done when
  - `bun test test/auth.test.ts` passes

  ## Plan (approved 21:20)
  1. …

  ## Report
  outcome done · "Login retries now back off; flaky test fixed" · tests 1/1 · decisions: Backoff cap = 30 s

  ## Reviews
  - 21:48 pass · `mise run verify` exit 0 · notes: none
  ```
- **The thread:** each launch appends the file's Brief and Done-when sections to the system prompt (`--append-system-prompt`); the plan, report and reviews stay out.
- **B1 and B2 now:** `thread_start` gains an optional `done_when` (commands that must pass); the review runs them beside the project's check and reads the brief.

## Reason
"Brief and done-when at launch"; "Both now"; on the format he asked "maybe backlog is better approach or? what makes sense more for you", and the agent's recommendation, our own format, was taken.

## Consequences
- Amends 0513: the task file, B1 and B2 move into v1; tasks as code stay after v1.
- `/handoff` still doesn't start from the task file (0513).
- `done_when`'s grammar is the evals' check grammar (0513); the review's proof (0453) includes each check's command and exit code.
- `thread_start`'s new parameter claims a name in plan appendix I.
