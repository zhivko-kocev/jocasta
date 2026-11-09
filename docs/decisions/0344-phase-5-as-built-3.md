# 0344 Phase 5 as-built, last items

- Status: decided
- Kind: product
- Phase/slice: 5/5.2, 5.5, 5.9
- Date: 2026-10-08

## Question
Are the last choices of the phase 5 build accepted?

## Facts
- Built and verified offline: 516 tests pass; the compiled `jarvis.exe memory import` runs; billion-context's own summary calls reach the fake Anthropic endpoint through the finisher with Claude Code's shape.
- Pi 1.1.0's Anthropic model list has no release dates; its newest ids are `claude-sonnet-5-5` and `claude-haiku-5-5`.
- Claude Code 2.1.295 ships as native binaries, so the SDK version it bundles can't be read from npm.

## Options
| Option | Pros | Cons |
|---|---|---|
| Accept as built | Done | — |
| Rework the unticked ones | — | — |

## Recommendation
Accept as built.

## Choice
- `graph_search(query, scope?)` searches the thread's project graph by default in a project's threads, and the personal graph elsewhere; one graph per search.
- The newest Sonnet and Haiku are the highest version numbers in the ids among models he is logged in to; with no Haiku login, decisions aren't generalized. Now (0377): Anthropic first, else GitHub Copilot; dotted ids count.
- A generalized decision that still names the project or a path is dropped before he sees it; other names rely on the prompt and his review.
- Removing a personal line doesn't delete it from the personal graph.
- The Anthropic account id found at login is stored in Pi's `auth.json` credential as `accountId`. Now (0377): dropped.
- Jarvis claims Claude Code 2.1.295 with the SDK version header kept at 0.112.1. Now (0377): dropped; the package claims its own version.
- Requests Jarvis didn't shape (billion-context's summaries) get Claude Code's identity, metadata and utility beta list in the finisher. Now (0377): dropped with the finisher.

## Reason
All ticked.

## Consequences
- None beyond the build.
