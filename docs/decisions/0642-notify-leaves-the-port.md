# 0642 notify leaves the port; the session watcher keeps its timer

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
0639 chose notify 8 to watch files. Porting showed that no TypeScript module watches files. Does notify stay, used for the session watcher, or leave? He asked which is better for the final picture, not for the project's current state.

## Facts
Verified 2026-11-15 in the TypeScript sources and the port.
- **No module watches files.** The session watcher (`watcher.ts`) scans omp's session folder every 30 s. herdr's `watch` is a socket subscription. Jocasta's other timers poll herdr, the clock, the quota, the machine and the pollers.
- **The scan is cheap.** It reads each session file from its last offset. His session root holds 139 session folders today.
- **History is the only thing that waits on the scan.** Its summaries feed `history_search` (0603). The boss reads them later, in another session, so 30 s never reaches him.
- **A file watch still needs the timer.** `ReadDirectoryChangesW` drops events when its buffer overflows, and sees nothing while Jocasta is down or the machine sleeps.
- **The port keeps the timer.** The Rust watcher passes the 13 ported cases of `watcher.test.ts`.

## Options
- **Remove notify; the watcher keeps its 30 s timer.**
  - Pros: same behaviour as the TypeScript build, one path, fewer crates.
  - Cons: amends 0639.
- **Use notify: a change in the session folder starts a scan at once, with the timer as a fallback.**
  - Pros: summaries reach history sooner.
  - Cons: two paths to one result, more code and tests, and a gain no reader sees.

## Recommendation
Remove notify. A future feature that must react to a file within a second, such as a live `guard.yml` reload or following omp² session files, brings it back with its own record and the 7-day rule.

## Choice
notify leaves `Cargo.toml`. The session watcher keeps its 30 s timer.

## Reason
His answer: `drop`.

## Consequences
- Amends 0639: notify is no longer one of the port's crates.
