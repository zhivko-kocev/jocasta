# 0502 H4 runs on his PC, in a sandbox

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-27

## Question
H4 (memory isolation: plant 50 facts in one project, then query 100 times from another) needs Graphiti, which runs in Docker. The VM has no Docker. Where does H4 run?

## Facts
Verified 2026-10-27.
- The design behind 0482 runs H1–H8 "in a throwaway herdr session on a sandbox build", all feasible on his machine; 0482 ties only H9 to the VM. `bench/README.md` had listed H4 under the VM.
- Docker Desktop in the VM needs nested virtualization, which he turns on with the VM off (`Set-VMProcessor -VMName jocasta-clean -ExposeVirtualizationExtensions $true`, as admin on his PC), plus an install 0497 doesn't cover.
- His PC has Docker Desktop. The campaign's sandbox ran Graphiti there under its own compose project (PerfReliability).

## Options
| Option | Pros | Cons |
|---|---|---|
| His PC, in a sandbox | What the design planned; nothing to switch on or install | Docker Desktop runs on his PC for about an hour; Graphiti's extraction uses his subscription |
| The VM, with Docker | Every harness number on one clean machine | His admin step with the VM off; Docker Desktop in the VM; Graphiti takes a few GB of the VM's 8 |

## Recommendation
His PC, in a sandbox.

## Choice
H4 runs on his PC in a sandbox Jocasta with its own data folder, herdr session and compose project. His Graphiti, the `jarvis` compose project and his herdr sessions are never touched.

## Reason
"Your PC, in a sandbox".

## Consequences
- `bench/README.md`'s table puts H4 on a sandbox Jocasta on his PC, with Docker.
