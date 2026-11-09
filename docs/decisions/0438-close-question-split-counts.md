# 0438 The close question counts working and awaiting threads apart

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
The close guard (0434) says "Stop N working threads?" while it also counts threads awaiting him. What does it say?

## Facts
Verified 2026-10-15.
- `busyThreads()` counts threads working and threads awaiting him together; the dialog calls them all "working" (`src/app/main.ts`).
- A thread awaiting him isn't working; its question is open in its pane.

## Options
| Option | Pros | Cons |
|---|---|---|
| Split the counts | True words for each kind | Three texts |
| Say "busy" | One text | Vague |
| Keep it | No change | Wrong for awaiting threads |

## Recommendation
Split the counts.

## Choice
The guard's text (dialog title "Jarvis", buttons unchanged): working only "Stop N working thread(s)? Their sessions are saved."; awaiting only "Stop N thread(s) awaiting you? Their sessions are saved."; both "Stop N working thread(s) and M awaiting you? Their sessions are saved." (singular "thread" for 1).

## Reason
"Split the counts".

## Consequences
- Amends 0434's question text.
- Amended by 0519: the question, with these texts, shows in Jocasta's own window, Keep the default and on Escape.
