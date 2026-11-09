# 0148 Edit-requires-read

- Status: dropped
- Kind: product
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
Must a file be read before it is edited?

## Facts
- Pi 1.0.1 has no such check.
- Lesson L69 stamped `(mtime, size)` at read and refused an edit to an unread or changed file on Pi 1.0.1: "refused to change src/math.ts: you have not read it in this session".
- omp tracks seen line ranges (`edit.enforceSeenLines`, `packages/coding-agent/src/edit/settings.ts:142-145`).
- With hashline (0146), every edit cites tags from a read.
- Appendix G: edits to unread lines are refused.

## Options
| Option | Pros | Cons |
|---|---|---|
| On, with the file stamped at read | Catches edits on unseen or changed files; cheap; hashline covers lines | An outside change forces a re-read |
| On, seen lines only | Precise | Duplicates hashline; more state |
| Off | No refusals | Blind overwrites |

## Recommendation
On, file stamped at read.

## Choice
Now (0307): dropped; hashline tags (0146) already make a stale `edit` fail.

Originally: A read stamps the file's mtime and size for the thread; editing or overwriting an existing file needs a current stamp, else the tool refuses and says to read it first.

## Reason
Not given.

## Consequences
- Writing a new file needs no stamp.
- Amended by 0307: dropped in the pruning pass.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
