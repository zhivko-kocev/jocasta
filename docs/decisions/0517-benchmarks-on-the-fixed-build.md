# 0517 Jocasta's benchmarks run on a build with herdr's dropped reads retried, and the done ones run again

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-01

## Question
Under bursts of thread starts and closes, herdr 0.9.5 sometimes accepts a request and closes the connection without answering. Jocasta's herdr client doesn't retry, so a `thread_start` fails ("herdr closed the connection before answering pane.list"). v1 gets the fix in source either way. Which build do the benchmarks measure, with H5, H6 and two H2 passes already done on the release?

## Facts
Verified 2026-11-01 in the VM's sandbox.
- 3 drops in about 30 minutes of H2 (`agent.prompt`, `pane.list` twice); herdr's server kept running. One ended an H2 pass at trial 30; one ended H8 at its first start (H8 starts each run's threads at once).
- Every request opens its own connection (`src/frontier/server/herdr.ts` `Client.attempt`), and a close before the answer becomes the error "transport"; only "unreachable" was retried (by reviving the server).
- The task benchmark doesn't use the release (it runs `C:\jc\repo`'s evals), so the full run doesn't depend on the build.
- The bench scripts use the same herdr client from `C:\jc\repo/src`.
- 0482 asks for a record for any change to the procedure before the run.

## Options
| Option | Pros | Cons |
|---|---|---|
| Full run first, then all on the fixed build | No delay to the full run; every Jocasta row on the build that ships | H8 and H3 about 3 days later |
| Fixed build now, rerun all | One build for every Jocasta row, now | The full run starts about 2 hours later |
| Release, bench retries | Keeps the release's rows | A harness change; the release's bug stays in the numbers |
| As is | No change at all | H8 reruns until its runs complete |

## Recommendation
Full run first, then all on the fixed build.

## Choice
Fixed build now, rerun all:
- Jocasta's herdr client asks a read (`ping`, `workspace.list`, `pane.list`, `pane.get`, `pane.read`, `pane.process_info`) again once when herdr closes the connection before answering; a change is never sent twice. Built under 0363 as a fix restoring decided behaviour.
- The release is rebuilt with it and installed in the VM's sandbox, and `C:\jc\repo` gets the same client, so the bench and the product match.
- H2 (window in front and minimized), H5 and H6 run again on it, then H8 and H3; then the full run.
- The release's rows stay in the kit as a comparison, in folders named `-release`, each notes file naming its build.

## Reason
"Fixed build now, rerun all".

## Consequences
- Differs from the recommendation: the full run starts after the sandbox benchmarks.
- Amends 0482's procedure before the run, as 0482 asks of any change.
- Amended by 0520: every Jocasta row runs again on the build with 0518 and 0519, and this build's rows stay in the kit as `-0517`.
