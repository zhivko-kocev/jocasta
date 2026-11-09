# 0531 H8's 8-thread run runs once more on a clean sandbox, and running out of memory again is its result

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
H8's attended 8-thread run under 0530 ran out of the VM's memory and left no row. Does it run again, and on what?

## Facts
Verified 2026-11-02 in the VM.
- The run started at 20:57:31Z with setup's files in place: 8 start boxes allowed, plans approved, and within 2.5 minutes 5 of the 8 threads had committed while the two `shared.ts` renamers asked.
- At 20:59:42Z Windows logged a low virtual memory condition (System event 2004, the largest three `bun.exe` at about 600 MB each) and showed its "Out of Virtual Memory" popup; the commit limit is 18.3 GB (8 GB of memory and the pagefile). WerFault crashed at 20:59:45Z, the operator's `herdr workspace list` failed (exit 45), a `hunk session comment list` of Jocasta's review and the bench itself ended with exit code 9 and no output (21:00:12–13Z), so there is no row.
- A working thread held about 1.1 GB: its omp about 500 MB, its review's sub-agent about 280 MB, hunk's diff about 225 MB, whiteboard's server about 78 MB and `jocasta mcp` about 65 MB.
- 22 idle PowerShell tabs from earlier sandbox starts (`sandbox-up.ps1` runs `jocasta` under `powershell -NoExit`) held about 1.3 GB; closed, the commit fell from 17.5 to 16.2 GB with the 8 threads still open.

## Options
| Option | Pros | Cons |
|---|---|---|
| Rerun once on a clean sandbox | A fair 8-thread run on the VM as 0481 set it up, without the leftovers; out of memory again is the result | Up to about 60 minutes more before H3 and the full run; it may run out again |
| Keep this run as the result | No extra hour; H3 starts at once on the 8 threads open | It ran beside 1.3 GB of leftovers, so out of memory isn't cleanly Jocasta's; no 8-thread row |
| Raise the VM's pagefile, then rerun | 8 threads very likely fit, and H8 gets a full row | An environment change after H2, H5 and H6 ran; paging may slow later runs; differs from 0481's VM |

## Recommendation
Rerun once on a clean sandbox.

## Choice
H8's 8-thread run runs once more, on a sandbox started clean, with the idle terminal tabs of earlier sandbox starts closed first and the VM's commit sampled every 10 s beside it. If the VM runs out of memory again, that run is the 8-thread result: the low-memory events, the commit samples and the operator's log stand for the row. The pagefile stays as it is. The run that ran out of memory is kept as `h8-jocasta-n8-oom`, and H3 then runs on the threads the rerun leaves open.

## Reason
"Rerun once on a clean sandbox".

## Consequences
- Amends 0529 (the 8-thread run's rerun) and 0530 (the order: H8's 8-thread run, then H3 on its threads, then the full run, unchanged).
- Every sandbox start now closes the idle terminal tabs earlier starts left.
