# 0513 After v1, product: the review, the brief, tasks and external tools

- Status: after v1
- Kind: product
- Phase/slice: after v1
- Date: 2026-11-01

## Question
Which product proposals from the 2026-11-01 research go on the after-v1 list, each grilled in detail when its phase opens?

## Facts
Verified 2026-11-01 (`docs/research/enhancement-proposals.md` and its reports in `D:/thesis/sources/proposals-2026-11/`).
- The reviewer gets the approved plan, the project's rules and the lessons, never the boss's brief (`src/frontier/server/hunk.ts` `reviewPrompt`); a brief carries no checks, and 0248 put a "done check" after v1 until the evals show threads reporting done with checks failing.
- The brief isn't stored after it is sent (`service.ts`, thread start); the review proof flags added test skip/only markers but no type or lint escape hatches (`hunk.ts` `reviewProof`).
- HumanLayer's Stop hook runs the checks silently and re-engages the agent on failure; Jocasta can't hook inside omp (0386) and checks only in the review turn.
- HumanLayer's show-me skill (MIT) draws call trees, file trees and diff sketches; the boss has a whiteboard-review skill.
- Loka measured skills and CLIs against a tool-per-task design ("116 → 25" tool calls); the boss's MCP tools are a proxy holding the service's token, the boss key and his Allow/Deny (0484). Jocasta's pollers already read GitHub (`gh`) and Azure DevOps (`az`).
- Destiner calls plan mode legacy and human review dying, without reasons; A1 → A2 and A2 → A3 measure them (0511).

## Options
Each item was asked on its own with "nothing more" or "keep as is" beside it; his picks are below.

## Recommendation
The reviewer reads the brief, `done_when` checks, the escape-hatch flags, the check on idle, show-me's formats, review by risk; the boss's MCP tools kept as they are.

## Choice
After v1, to be grilled when their phase opens:
- **The reviewer reads his brief** beside the approved plan, checking the diff against the intent (amends 0453 then).
- **`done_when` checks** on `thread_start`, in the evals' check grammar, run in the review step (reopens 0248).
- **A task file per thread** in the data folder: brief, approved plan, each review's proof, outcome. Tasks as code (markdown tasks in the repo) only in his own new projects; a client project keeps its own setup and tracker (Jira for one client, something else for another), and the boss works with each project's own setup, agnostic of the provider.
- **The review proof flags escape hatches**: added `@ts-ignore`, `as any`, `biome-ignore`, `unsafe`, and edits to check configs.
- **A check on idle from the service**: when a thread goes idle with new edits, the service runs the project's check (`mise run verify`, 0473) and sends only the failures into the pane; silent on pass.
- **show-me's formats** in the boss's whiteboard-review skill.
- **Review by risk**: the machine reviewer always runs; his attention goes to blocking notes and the proof (diff size, test edits, escape hatches).
- **The plan gate by size**, decided after the full run from A1 against A2: small, clear briefs might skip plan mode (would amend 0285).
- **The boss through a CLI, measured against its MCP tools first**; then provider-agnostic commands for source control (GitHub through `gh`, Azure DevOps through `az`) and for tickets (Jira, GitHub Issues, Azure Boards), decided together after the measurement.

## Reason
"Reviewer reads your brief", "done_when checks on thread_start", "One task file per thread", "Review flags escape hatches"; "Service-side check on idle"; "show-me formats in the whiteboard-review skill"; "Plan gate by size, decided after the full run", "Review by risk"; "measure cli against mcp and we can also use cli for the other external tools we will have in future for example the jira or a tickets agnostic tool that unifies more clis from different providers or gh cli or dev azure cli"; "Both, decided together after the measurement"; on tasks as code: "that is for new projects that i decide but when i am workin on another project that project has its own setup and i must follow it so i want a full capable boss/orchestrator that can handle everything and be very agnostic so i can work on two clients one using jira the other one using smth else".

## Consequences
- Differs from the recommendation: the boss's tools are measured against a CLI, and the agnostic external-tool commands are new.
- Not chosen: a thread started from a fork of another's session, and the boss manual line for two designs; `/handoff` starting from the task file, and a backlog from inbox notes; typed herdr ids and a separate interface review.
- Nothing here is built in v1; each item gets its own records first (0002).
- Amended by 0540: the task file per thread, the reviewer reading the brief and `done_when` checks move into v1 with the harness-OS layer (0532).
- Amended by 0542: the check on idle moves into v1, run through the cached single-flight runner.
