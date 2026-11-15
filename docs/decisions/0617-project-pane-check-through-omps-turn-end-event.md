# 0617 A failed check reaches a project pane's agent through omp's turn-end event

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
0610 put the check on idle into project panes, with the failing gate typed into the pane as in threads. The build stopped itself (0020): in a pane he types in, typing is not safe. How does a failure reach the agent there?

## Facts
Verified 2026-11-14 on omp 18.20.0 and herdr 0.9.5 (report `hook-spike.md` in `D:/jocasta-spikes/sota/reports/`, outside the repo; run in a throwaway herdr session on the cheapest model).
- **Typing into a pane** is a paste followed by Enter. omp's editor puts a paste at the cursor and never clears the box, and Enter sends the whole box. Reproduced: text sent to a pane whose box held "draft" was submitted as one message beginning with "draft".
- **Nothing shows Jocasta that a draft is there:** omp's collab list and herdr's agent states have no such field, and the pane's screen text cannot tell omp's placeholder from a draft.
- **omp's `session_stop` event** is called for an extension before a session settles. A handler may answer `{continue: true, additionalContext}`: the model then takes another turn and sees the text as a hidden message. Observed: the model acted on it, nothing was typed, and a draft typed during the turn was still in the box, unchanged.
- **It is called for the pane's own session only**, never for a subagent's.
- **It is called again after every settle**, also one the handler or the advisor caused, so a handler has to answer once per tree itself. The event carries no folder and no sign of edits; the handler's context gives the folder.
- **omp ends a handler after 30 seconds:** one error line in the pane, and the session settles. A handler that throws gives the same line and no crash. At most 8 continues in a row are honoured (read in omp's source).
- **Loading:** an `extensions:` list in a `--config` settings file loads the file (observed). Jocasta already writes such a file for every project pane. Nothing goes into omp's own folders.
- **Cost:** token counts were identical with and without the extension. A run started 1 to 2 s slower in a small sample.
- **0386** says no Jocasta code runs inside omp. Its reason was a harness of Jocasta's own inside omp, rebased at every omp release. herdr's own extension already runs in his omp.
- **The need:** $704 of the window's $977 was spent in project panes and their subagents, and none of the 4 project-pane sessions that edited ran a check after its last edit. Jocasta's own suite takes about 100 s.

## Options
| Option | For | Against |
|---|---|---|
| omp's turn-end event, in project panes | the agent corrects itself; his draft is never touched; no fixed tokens | an exception to 0386; omp's event can change with a release; a check slower than 30 s is mostly not given to the agent |
| Shown in `today` only | no code inside omp | the agent never corrects itself |
| Threads only | nothing to build | the measured gap stays |

Typing it anyway was not offered: the spike showed it corrupts a draft.

## Recommendation
omp's turn-end event.

## Choice
- **Project panes load one small omp extension of Jocasta's**, named when Jocasta starts their omp (a settings file of Jocasta's or omp's `-e` flag: the way that leaves his own extensions loading). Nothing goes into omp's folders. It handles `session_stop` and nothing else.
- **When a turn ends, the handler asks the Jocasta service for the pane's check.** The service applies the threads' rule (0513, 0542): a tree with edits not yet checked runs the project's `verify` task, else `test`, through the cached runner.
- **A check that fails within the handler's time is given to the agent** as the turn's continuation, in the threads' frame text, once per tree. The agent keeps working.
- **A slower check lets the turn end.** If it fails, `today` shows a line, and the agent is given the failure at its next turn end if the tree has not changed.
- **A pass says nothing.** With the layer off, or with Jocasta not running, the handler does nothing.
- **Threads keep the typed way**, which is safe there. The boss pane gets no check. One way for both is asked after the benchmarks.
- **0386 gets this one exception:** one file, one event. Nothing else of Jocasta's runs inside omp.

## Reason
His answer: `hook`.

## Consequences
- Amends 0610 (how a failure reaches a project pane) and 0386 (the exception).
- An omp bump re-checks the event, its 30 s limit and the way the file is loaded (0383's list).
- In a project with a slow check, Jocasta's own among them, the agent mostly does not get the failure by itself; he sees it in `today`.
- Plan appendix I gains the extension file, the service's route and the settings key that loads it (0027).
