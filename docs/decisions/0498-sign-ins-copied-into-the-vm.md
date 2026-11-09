# 0498 His sign-ins are copied into the VM

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-27

## Question
omp and Claude Code in the VM need his Anthropic sign-in (his subscription, no API keys, 0482). Does he sign in inside the VM, or does the agent copy his PC's sign-ins?

## Facts
Verified 2026-10-27.
- omp keeps its sign-ins in `~/.omp/agent/agent.db`, table `auth_credentials`: his has two `anthropic` OAuth rows. Claude Code keeps its sign-in in `~/.claude/.credentials.json`.
- An OAuth refresh can replace the refresh token the other copy holds, and then that copy stops working until he signs in again there.

## Options
| Option | Pros | Cons |
|---|---|---|
| He signs in inside the VM when asked | His PC's sign-ins stay untouched; the tokens never pass through the agent | About 2 minutes of his time, once for omp and once for Claude Code |
| The agent copies his PC's sign-ins | No sign-in for him | A refresh on one side can sign out the other; his tokens pass through the agent |

## Recommendation
He signs in inside the VM when asked.

## Choice
The agent copies his PC's sign-ins into the VM: only omp's enabled `auth_credentials` rows (none of `agent.db`'s history, memories or usage) and Claude Code's `.credentials.json`. Nothing is printed, and the transfer file is deleted on both sides.

## Reason
"Copy my PC's sign-ins into the VM" (differs from the recommendation).

## Consequences
- Differs from the recommendation.
- When a refresh signs one side out, he signs in again there. The task benchmark stops after three sign-in failures in a row instead of scoring them as failed tasks.
- Amended by 0504: his sign-ins are made inside the VM, and nothing is copied any more (the copies broke within hours, both ways).
