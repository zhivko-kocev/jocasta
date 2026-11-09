# 0082 Catalog rows from Strands Agents

- Status: decided
- Kind: product
- Phase/slice: 0/appendix A
- Vision: V8
- Date: 2026-10-07

## Question
Which rows from Strands Agents join appendix A?

## Facts
Verified 2026-10-07 at https://strandsagents.com/.
- GoalLoop (`docs/user-guide/sdk/plugins/goal-loop/`) validates each response and re-invokes the agent with feedback until it passes, hits `maxAttempts` or times out; a function validator such as `npm test` "skips the judge agent entirely".
- Interrupts (`docs/user-guide/sdk/interrupts/`): "The agent stops its loop and hands control back to you along with the pending interrupt. You provide a response, and the agent resumes from the point where it stopped."
- ContextInjector (`docs/user-guide/sdk/plugins/context-injector/`): text "added to that single call's input only: it is never written to the conversation history, so it is not persisted or replayed on later turns."
- Box (`docs/user-guide/box/`): Dogwood rules "can depend on what the agent already did, in what order, and how long ago"; Box is pre-release 0.1.x and runs on macOS 15 or later on Apple silicon only; Windows is unsupported.
- Strands Shell lists macOS, Linux and WASM, with no native Windows build (`docs/user-guide/shell/how-it-works/`), so it was not offered.
- V8: candidates come from "omp, Claude Code, opencode and others". Appendix A named omp, Claude Code, opencode and Pi as its sources.
- Plan before this record: 3.6 headless runs return an error; stop detection is mechanical (appendix G); 3.3 places the briefing at the window's start or as a mid-conversation system message, plus 0077's context epochs; 2.1 rules judge one call at a time.

## Options
| Option | Pros | Cons |
|---|---|---|
| Done check (3.7) | A workspace thread can't report done while its tests fail; mechanical (principle 2) | Needs a check command per brief; retries cost tokens |
| Resumable headless questions (3.6, 1.7) | A headless or boss-run thread waits for an answer instead of failing | Run state must survive between the stop and the answer |
| Per-request context (3.3) | Clock and environment facts reach the model without entering history or the cached prefix | A third placement option next to the briefing and context epochs |
| History-aware rules (2.1) | Rules like "no commit unless the tests passed after the last edit" | The engine keeps per-run history; more complex than per-call rules |
| None | — | — |

## Recommendation
Done check, resumable questions and per-request context; history-aware rules optional.

## Choice
All four.

## Reason
Not given.

## Consequences
- Appendix A gains four rows: done check (3.7), resumable questions and approvals (3.6, 1.7), per-request context (3.3) and history-aware rules (2.1).
- Appendix A's sources line names the other harnesses a row cites (V8: "and others").
- The choice adds history-aware rules, which the recommendation left optional.
