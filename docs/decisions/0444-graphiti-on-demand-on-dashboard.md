# 0444 Graphiti on the dashboard says it runs on demand

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
Graphiti starts when Jarvis first needs it and stops after 15 min idle (0073), so the services row reads a red "down · not started" most of the day. What does it show?

## Facts
Verified 2026-10-15.
- On 2026-10-15 Graphiti ran 11:14–11:33 for the waiting summaries, then stopped after 15 min idle, as 0073 says; the row showed "down · not started" in red the whole time it wasn't running.
- The same day's real failure was Docker Desktop not running (its start at sign-in is off): every start failed with "failed to connect to the docker API".
- Whether Docker's engine is running can be checked without starting anything: its named pipe (`\\.\pipe\dockerDesktopLinuxEngine` or `\\.\pipe\docker_engine`) exists only while it runs.

## Options
| Option | Pros | Cons |
|---|---|---|
| On demand, say why | True words for each state; red only when broken | Four states instead of two |
| Keep it running | "Up" means up | Containers hold memory all day; amends 0073 |
| As now | No change | Red while nothing is wrong |

## Recommendation
On demand, say why.

## Choice
The services row says for Graphiti: "up" while it answers; "starting" while Jarvis starts it; "idle · starts when needed" (not red) while it isn't running and Docker is; "down · Docker isn't running" (red) while Docker's engine isn't running; "down · not answering" (red) when it runs but doesn't answer. 0073's start on first use and 15-min idle stop stay.

## Reason
"On demand, say why".

## Consequences
- Amends 0429's services row ("whether Graphiti, whiteboard and herdr are up").
