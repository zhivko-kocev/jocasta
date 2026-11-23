# 0662 A service that stops answering is shown in the dock, and the boss's calls to it end

- Status: v1
- Kind: product
- Phase/slice: fork build, after his first day on it
- Date: 2026-11-23

## Question
On 2026-11-23 the service hung for over an hour (ledger row 107) and nothing told him: the dock kept its last picture, and the boss's calls waited until omp's limit. What should show?

## Facts
Verified 2026-11-23:
- The dock shows "Jocasta isn't answering" only on the service's goodbye (`ServerMessage::Bye`, `src/frontier/shell.rs`); the service pushes messages only on change, so silence looks like a quiet day.
- `jocasta mcp`'s `call_service` (`src/mcp/server.rs`) has no client cut (0577), and setup gives omp's `jocasta` servers 1,860,000 ms and the boss's 2,100,000 ms, because `check`, the reviews and his Allow wait that long.
- In the boss's log of that day, `mcp__jocasta_projects` calls timed out while the service accepted connections and never replied.

## Options
- **Both**: the dock says "Jocasta isn't answering (since HH:MM)" after 30 s without word, and `jocasta mcp` ends a call after 60 s, except calls that may wait for his Allow or a run. Pros: he and the boss both learn within a minute. Cons: a heartbeat from the service, and a per-tool rule.
- **The dock only.** Pros: smaller. Cons: the boss still waits up to 35 minutes.
- **Neither.** Pros: nothing to build. Cons: the next hang is as silent.

## Recommendation
Both.

## Choice
His answer of 2026-11-23: both.
- The service sends the dock a heartbeat every 10 s from its refresh tick, after the tick has read the day's state, so a service stuck on that state stops beating. With no message for 30 s the dock and the HUD show `Jocasta isn't answering (since HH:MM)` and the dock keeps its last picture dimmed; the next message clears it.
- `jocasta mcp` ends a call with no answer after 60 s with `<tool>: Jocasta's service didn't answer in 60 s. Close and start Jocasta again if this repeats.`, as an error result. Tools that may wait for his Allow, a check, a review or a commit keep 0577's no-cut rule.

## Reason
His answer above.

## Consequences
- Amends 0577 (no client cut, now only for the waiting tools) and 0581 (UX-17: offline shown on goodbye only).
- The plan's appendix I notes the 60 s cut beside 0577's timeouts.
