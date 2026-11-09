# 0509 Session state follows omp's active branch

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-11-01

## Question
omp keeps a session as a tree, and a rewind leaves the abandoned turns in the file. Jocasta reads every line in file order. Does the state the service reads follow the active branch, and when is it built?

## Facts
Verified 2026-11-01 in omp 18.13.1's installed source and in Jocasta.
- omp sets the leaf to each replayed entry's `activeLeafId`, else its id (`session-entries.ts` `sessionEntryLeafId`); `branch()` and `branchWithSummary()` move the leaf back, and a new entry takes the leaf as its `parentId` (`session-manager.ts`). omp's own state comes from the walk back from the leaf (`getBranch()`).
- Jocasta's `addLine` (`src/frontier/server/sessions.ts`) folds every line in file order; `parentId` appears nowhere in it. So after a rewind, an abandoned branch's `TESTS: pass` (0453), plan proposal (0285, 0494), sent-back count (0471), bash runs, question or last line counts until the new branch writes a newer one.
- The trigger is a rewind across a review or a plan approval: rare; the review's `since` bound narrows it.
- Stencil's playbook names the same failure in Pi's plan-mode example: "plan mode restored from the whole file, not the selected branch" (https://stencil.so/blog/harness-playbook).

## Options
| Option | Pros | Cons |
|---|---|---|
| Before the cutover | v1 ships with 0453's proof right after rewinds; the cutover builds from source anyway | Changes the core session reader after the VM test |
| After v1 | v1 stays as tested in the VM | The bug waits for a rewind across a review |

## Recommendation
Before the cutover.

## Choice
Before the cutover:
- cost, spend and message times stay in file order, since abandoned turns were billed;
- the plan state, the test result, bash runs, the question, the last line, the model and the handoff time follow the active branch as omp replays it: each entry's `activeLeafId`, else its id, is the leaf, and the branch is the walk back by `parentId`;
- an entry that extends the leaf applies at once, an off-branch entry changes nothing, and any other leaf move replays the branch from its root;
- entries without an id or a `parentId` keep file order.

## Reason
"Before the cutover".

## Consequences
- Tests: a rewound plan proposal leaves no plan waiting; a rewound review's `TESTS: pass` doesn't count; billing counts both branches.
- A sandbox smoke before the cutover; the cutover's build carries it (0487).
- Found by the 2026-11-01 research (`docs/research/enhancement-proposals.md`, finding 2).
