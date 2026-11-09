# 0583 The boss manual and shipped prompt texts

- Status: v1
- Kind: product, prompts
- Phase/slice: program
- Vision: V8, V11
- Date: 2026-11-09

## Question
The review found that the boss manual still speaks of spoken events and of tools omp no longer names that way, its review vocabulary is stale, the weekly drift review goes to a bug-hunting agent, and several shipped texts contradict each other or are cut. What do the texts say?

## Facts
Verified 2026-11-09 by the review (0573), report PRM.
- `config/boss-manual.md:58`: "Each was already spoken and shown to him … use speak only when he must hear it now"; voice and `speak` went with 0552, and only awaiting-you, plan-ready and done events notify him.
- Manual line 16 says the tools are "already loaded … named `mcp__jocasta_<tool>`"; omp 18.18.2 mounts MCP tools as `xd://` devices by default (`tools.xdev`).
- Line 39's "done and reviewed: N notes, tests pass" predates the real event (commits, size, blocking notes, tests unverified, done-when exit); line 58's list misses "finished its turn".
- The weekly prompt runs omp's `reviewer`, whose own prompt hunts bugs; the mise-setup brief says "don't change the repo" while every brief ends with the commit line.
- Idle-check failures are typed into the thread as a bare gate block; omp's catalog cuts MCP descriptions at 200 B, and 7 of 16 boss descriptions are single lines over it.

## Options
| Item | Options (pros / cons) |
|---|---|
| PRM-4 event sentence | (a) rewrite it · (b) delete the voice clauses only (keeps "shown to him") · (c) also notify "finished its turn" |
| PRM-8 tool calling | (a) reword · (b) inline Jocasta's schemas (~8 KB) · (c) `tools.xdev: false` (every tool inline) |
| PRM-10 review vocabulary | (a) rewrite line 39 with a pass rule · (b) put the verdict in the event (changes an H2-timed text) |
| PRM-14 drift review | (a) omp's `task` agent · (b) a shipped `drift` agent (can pin the slow model) · (c) tell the reviewer to set its criteria aside |
| PRM-15 mise setup | (a) the brief says no commit · (b) a `thread_start` flag |
| PRM-16 idle checks | (a) framed with a stop rule · (b) not typed · (c) skip after a "?" |
| PRM-17 descriptions | (a) two-line descriptions · (b) leave them |
| PRM-18 small texts | fix (a)–(e) · leave them |

## Recommendation
(a) for each; PRM-14 (b) when the slow model is kept; PRM-18 all five.

## Choice
- PRM-4 (a): "Awaiting-you, plan-ready and done events also reached him as a notification; the others reach only you. Answer an event only when it changes what he should do next (always a thread's question, even one in a finished turn's last line), in one short line that adds something; otherwise end your turn without a reply."
- PRM-8 (a): "Your tools are the `jocasta` MCP server's, mounted by omp as `xd://mcp__jocasta_<tool>`: to call one, write its JSON arguments (listed below) to that path. The lists below are complete, so you needn't read a device's docs first."
- PRM-10 (a): line 39's tail becomes "…and you hear `done and reviewed: <commits>, <size>, N notes (M blocking), tests pass|fail|unverified|no tests[, done-when <cmd> exit N][, no report]`. It passed only when tests pass (or there are none), the notes were counted, none is blocking and every done-when exited 0; otherwise say which part failed."; line 58's list gains "a turn that ended without new commits (its last line; a question in it needs him)".
- PRM-14 (b): a small shipped `drift` agent definition written beside the lessons skill (it can pin the slow model): "Review a week of merged changes for design drift: a second way of doing what already has one, patterns that disagree, broken layering, a rule or lesson contradicted. Not bugs."; the weekly brief says "Run the drift subagent on that range."
- PRM-15 (a): the mise-setup brief ends "This task changes no repo file: make no commit (the commit step doesn't apply)."; the skill tells the boss to report the thread's "finished its turn" line as its outcome and offer to close it.
- PRM-16 (a): idle-check failures are typed with the frame "Jocasta's checks failed on your uncommitted changes (this isn't his message). If you're waiting for his answer, end your turn now; otherwise fix these before you commit:" then the gate block.
- PRM-17 (a): each MCP tool description gets a short first line (≤ 80 B), a newline, then the details.
- PRM-18: (a) thread_start's description: "a worktree workspace under its project's, running omp in plan mode"; (b) the leftover-worktrees skill: "N ahead, clean or M changed"; (c) the lessons skill: "Only on his word: when he says "make that a lesson" or says yes to the boss's offer.", and the shipped header is refreshed on every ensure; (d) the diagnose skill: "`<UTC time> <LEVEL> …`; his times are local, so convert before you look."; (e) the manual names his own doors: "He can also act without you: Ctrl+B then Alt+F finish, Alt+S stop, Alt+R review, Alt+N new thread; `jocasta thread …` in a terminal."

## Reason
"(a)" for PRM-4, PRM-8, PRM-10, PRM-15, PRM-16 and PRM-17; "(b)" for PRM-14; PRM-18 ticked.

## Consequences
- Amends 0296/0405/0413 (spoken events, gone with 0552), 0453 (the review's pass rule), 0460 and 0470 (the weekly drift review's agent), 0465 (the manual and the tool descriptions), 0471, 0472, 0477 (mise setup), 0513 (idle-check failures), 0541 and 0546.
- The shipped `drift` agent is one more shipped prompt, written where the lessons skill is.
