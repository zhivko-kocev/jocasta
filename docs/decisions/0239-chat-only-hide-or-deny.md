# 0239 Chat-only tools declared and denied

- Status: decided
- Kind: product
- Phase/slice: 2/2.7
- Date: 2026-10-08

## Question
Are write tools hidden in chat-only threads, or declared and denied?

## Facts
- Lesson L36: changing the tool list changes the first block of every request and breaks the prompt cache; opencode denies at call time to keep the list stable.

## Options
| Option | Pros | Cons |
|---|---|---|
| Declared and denied by the guard's `tool_call` handler | Stable tool list; no new mechanism | The model may try a denied tool |
| Hidden | The model never sees them | Cache break on every switch |

## Recommendation
Declared and denied.

## Choice
Declared and denied: the tool list is the same in every thread, and the guard denies with the chat-only message.

## Reason
Not given.

## Consequences
- None.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
