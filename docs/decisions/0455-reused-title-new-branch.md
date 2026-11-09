# 0455 A reused title gets a new branch

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
A thread started with the title of an earlier thread reuses its folder. What branch and session does it get?

## Facts
Verified 2026-10-23.
- A freed folder is reused (`service.ts:177-183`), and panes start with `omp --continue` (`service.ts:415,1710`), which resumes the newest session saved for that folder (omp `session-manager.ts:4433`), so a new thread can resume the old thread's session.
- After a PR the kept `feature/<slug>` branch stays, and herdr checks out an existing branch as it is, old commits included (socket-api.mdx:344).
- Read in code, not run live.

## Options
| Option | Pros | Cons |
|---|---|---|
| New branch `-2`, `-3`… like its folder, fresh session | Never picks up old work | Branch names drift from the title |
| Refuse, naming the old branch | Explicit | He renames or deletes first |
| Keep as today | No change | Old conversation and commits, silently |

## Recommendation
New branch `-2`, `-3`… like its folder, fresh session.

## Choice
When `feature/<slug>` exists, the new thread's branch takes `-2`, `-3`… like its folder (0445), and a new thread's pane starts a fresh session (never resumes an old one).

## Reason
"New branch `-2`, `-3`… like its folder, fresh session".

## Consequences
- Amends 0414 (the branch name) and 0445 (the suffix applies to the branch too).
- Reopening an existing thread still runs `omp --continue` (0399).
