# 0504 His sign-ins are made inside the VM

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-28

## Question
The VM's copied sign-ins already broke: his PC's omp refreshed its tokens, and the VM's copies were then rejected. The benchmark runs for hours, and each refresh on one side can sign the other out, his PC's omp or Claude Code included. How should the VM sign in? And opencode, which H6 measures after one prompt?

## Facts
Verified 2026-10-28.
- His PC's omp refreshed its two Anthropic sign-ins at 00:57Z. The VM's copies (made on 2026-10-27) failed their refresh at 01:30Z ("invalid_grant", "Refresh token not found or invalid"), and omp disabled both; his PC's stayed active.
- The agent's one-word Claude Code check in the VM at 01:25Z refreshed the copied sign-in (his PC's had expired at 16:47Z the day before), so his PC's copy could no longer refresh. On his choice the agent copied the VM's refreshed sign-in back to his PC, deleted the VM's copy, and deleted every copied omp row in the VM (users `tester` and `tester2`).
- A task run lasts hours and an access token far less (Claude Code's fresh one: 8 hours), so both sides refresh during a run.
- opencode 1.18.22 in the VM has no sign-in (0503); his PC's has none either.

## Options
| Option | Pros | Cons |
|---|---|---|
| He signs in inside the VM | Separate sign-ins: neither side can sign the other out | About 3 minutes of his time at the VM's console |
| The agent copies again before each run | No time from him | A run breaks when his PC refreshes; a refresh in the VM signs his PC out |
| One more copy, for the short steps only | H1's live check and the harness benchmarks start now | The same risk for an hour or two; he signs in before the pilot anyway |

For opencode: sign it in inside the VM too; measure it idle only; or copy his PC's sign-in.

## Recommendation
He signs in inside the VM, opencode included.

## Choice
He signs in inside the VM, at its console as `tester`: omp's `/login` for his Anthropic accounts, Claude Code's `/login`, and opencode's sign-in. No sign-in is copied between his PC and the VM any more.

## Reason
"You sign in inside the VM"; for opencode, "Sign it in inside the VM too".

## Consequences
- Amends 0498: the copying ends, and so does its risk.
- Amends 0503: opencode is signed in, so H6 measures its one-prompt row like the others.
- H1's live check, H2, H3, H5, H6, H8 and the task runs wait for his sign-in.
- Amended by 0506: opencode isn't signed in after all (its Anthropic sign-in takes only an API key); omp and Claude Code are.
