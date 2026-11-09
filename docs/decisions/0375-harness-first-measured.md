# 0375 Harness first, measured

- Status: decided
- Kind: process
- Phase/slice: program (all)
- Date: 2026-10-10

## Question
After the v1 beta 2, what does Jarvis work on next, in what order, and how is each change judged?

## Facts
Verified 2026-10-10.
- The prompt text is still Pi's builder and sections, grown from 2.76K to 3.5K characters. The load sent with each request is not: bare Pi sends 4 tools and ~1,370 tokens; a Jarvis workspace thread sends 19 tools and ~6,030 tokens (4.4×); with billion-context, 24 tools and ~10,200 tokens (7.4×).
- The biggest tools in tokens: `draw` 1,867, ask 359, subagent 349, browser 343, graph_search 202. Pi ships `tool_search` and `codemode`, both inactive in Jarvis.
- The evals have 2 placeholder cases (`fix-sum`, `write-notes`); a row records the main session's usage only. Noise at 2 cases × 3 reps is ±41 points. 0320's benchmark adapters were never built.
- Docker, Python, uv and WSL Ubuntu are present; Harbor runs Terminal-Bench 2.0 with any agent that installs in a container.

## Options
| Option | Pros | Cons |
|---|---|---|
| The harness first, each slice measured on the evals, then Frontier | Simple, reliable and fast before it grows; each change is kept on numbers | Frontier waits |
| Frontier first | The visible part grows sooner | The harness under it stays unmeasured |

## Recommendation
The harness first, measured.

## Choice
The program runs in this order: A1 → A2 → A3 → A4 with A5 → A7 → G2 → B1 → C1 → C2 → C3 → C4 → D with R → E → A6 → B2 → F → G1 → H.
- A1 records and the beta-2 leftovers (0382); A2 the fake provider leaves the product (0376); A3 Anthropic through pi-anthropic-auth, Copilot first (0377); A4 Jarvis everywhere (0378); A5 tool guidance (0381); A7 the beta-2 leftovers he ticked; G2 the canvas like whiteboard (0379); B1 the benchmark loop; C1 roles; C2 plan mode; C3 `/pause`; C4 LSP; D code review with R, the diff after every task (0380); E Ponytail; A6 the prompt diet, billion-context off included; B2 Terminal-Bench 2.0; F memory; G1 the boss chat, opencode-simple; H claude-code-best-practice's tips, worktree task and the finish flow.
- Each slice runs one loop: grill only what goes beyond its records (0363), with the interactive question tool; write the records; build; run the checks; run it on the real model; from B1 on, run `bun run evals` before and after and keep or revert by the rule below.
- The keep rule, read by `bun run evals compare <runDirA> <runDirB>`:
  - the pass rate holds when rate B ≥ rate A − √(100²/nA + 100²/nB) points;
  - a metric improves when the mean over cases of (mean B ÷ mean A) is ≤ 0.90 for tokens, wall time or lines changed; a case whose A mean is 0 is skipped for that metric;
  - the verdict is "keep" when the pass rate holds and at least one metric improves, otherwise "revert". A slice that adds what he asked for is kept by his word whatever the verdict, and the verdict is reported to him.
- The loop runs on Jarvis's own suite first, then Terminal-Bench 2.0.

## Reason
His direction after the v1 beta 2: Jarvis is bare Pi tweaked with the best of the harnesses he has used (omp, Claude Code, opencode); it stays simple, reliable, fast and good on benchmarks, and only then does Frontier grow further.

## Consequences
- B1 adds ten real-mode cases, variants (`--variant <name>`, `evals/variants/`), per-trial metrics, a usage ledger (`JARVIS_EVAL_USAGE_FILE`, tests and evals only) and `bun run evals compare`; run dirs become `<time>-<label>` (amends 0166).
- Adopted from omp: model roles, plan mode, LSP and `/pause`; from Claude Code: code review; from opencode: its simplicity, for the boss chat only. The canvas stays fancy.
- The Jarvis header and Graphiti stay; memory is strengthened later (F). Ponytail is tried and its bloat measured (E).
- Amended by 0384: the program pauses after B1 for F0, F1, U, F2, F3 and F4 on upstreams; E, A6, B2, G1 and H resume after F4 where they still apply.
- Amended by 0421–0426: E is an eval variant; A6 dropped; B2 after v1; G1 is the boss's config; H is tests in review and thread_finish.
