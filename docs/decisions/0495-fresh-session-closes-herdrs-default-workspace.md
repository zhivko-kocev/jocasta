# 0495 A fresh session closes herdr's default workspace

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
Every new herdr session Jocasta creates also gets herdr's own default workspace, a PowerShell in his home folder, which Jocasta never uses. Close it, or keep it?

## Facts
Verified 2026-10-24 in the campaign (0481), on sandbox builds.
- The workspace is named after his user, holds one PowerShell pane in his home folder, takes focus when he attaches, and costs about 70 MB (performance slice).
- It made every first `thread_start` of the beta day ask for his Allow, until 0492 stopped panes in no project from counting.
- It was first seen in the polish round and left as herdr made it.

## Options
| Option | Pros | Cons |
|---|---|---|
| Close it on a fresh session | He attaches straight to the Jocasta workspace; no idle shell | Jocasta closes a workspace herdr made |
| Keep it | herdr as it ships | An idle shell in every session, focused at attach |

## Recommendation
Close it on a fresh session.

## Choice
When Jocasta has just created herdr's session, it closes herdr's default workspace once its own workspaces exist, only while that workspace is untouched (its one pane a shell in his home folder, no agent, nothing typed). A session it didn't create, and any workspace he made, is never touched.

## Reason
"Close it on a fresh session".

## Consequences
- Amends 0392 (what Jocasta's herdr session holds).
- Appendix I's herdr paragraph says so.
