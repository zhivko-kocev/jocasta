# 0559 mise's task cache keeps passing checks; Jocasta keys results by git tree, keeps failures and runs one check per tree

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V11
- Date: 2026-11-09

## Question
0542's spike: does mise's experimental task cache give Jocasta cached checks, including the check on idle, and single-flight per git tree? What must Jocasta add?

## Facts
Observed 2026-11-09 with mise 2026.10.17, Bun 1.4.3 and Node 24.21.0 on a synthetic fixture with a second worktree (`D:/jocasta-spikes/mise-cache/`; report kept with the session's spike notes).
- With `experimental = true` and `cache = { enabled = true }`, a task is keyed by the contents of its `sources` plus `mise.toml`, the task definition, tool versions and platform; a hit replays the saved log and exits 0. A full check took 0.56–0.76 s; touching files, switching branches, reverting an edit or running in a second worktree hit the cache at 0.12–0.16 s.
- Failed runs are never stored: a failing tree reruns its failing tasks on every check. Without `-c`, the first failure aborts the passing tasks and their results are lost.
- Two concurrent `mise run check` on one tree both ran every task; the `lock` field on mise's website is refused by 2026.10.17 as unknown.
- A tested sketch: a key from the git tree (copy of the index, `git add -A`, `git write-tree`; 84–138 ms), an exclusive lock file per key with waiters joining the running check, and stored exit code and log for PASS and FAIL; a second caller joined the first run and a cached failure replayed with exit 1.

## Options
No choice for him: 0542 already decided cached checks with single-flight by git tree; the spike shows which part mise gives and which Jocasta adds (0363).

## Recommendation
As the choice.

## Choice
- Jocasta keeps one result per git tree (the temporary-index `write-tree` key, plus the tool versions), PASS and FAIL alike, with its exit code and log; a check whose key already has a result answers from it, so the check on idle costs one key computation when nothing changed.
- One check runs per key: an exclusive lock file, waiters joining the running check's result, a lock whose process is gone taken over.
- The check runs `mise run -c check`, with mise's artifact cache switched on in the project's `mise.toml` underneath, so tasks whose sources didn't change are replayed.
- Checks answer as 0546's gates.

## Reason
The spike's numbers; 0542's choice.

## Consequences
- Carries out 0542; the experimental cache needs `experimental = true` in the project's `mise.toml`, and its key format may change with a mise bump (0383).
- Every task must list all its inputs in `sources`; Jocasta's tree key covers the repo's files but not ambient environment.
- Left on disk: `D:/jocasta-spikes/mise-cache/` only (fixture, worktree, isolated mise state and cache); no install, no `mise trust` entry, nothing running.
