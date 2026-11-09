# 0505 PowerShell 7 is installed in the VM

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-28

## Question
52 of H1's git lines run inside `pwsh -c "…"`. PowerShell 7 isn't in the VM, so omp's real git guard couldn't be measured on them. Install PowerShell 7 in the VM?

## Facts
Verified 2026-10-28.
- `git-guard-check.ts` measured 318 of the corpus's 423 git-only lines, and the reimplementation (`git-guard.ts`) agrees on all 318. Not measured: the 52 `pwsh -c` lines, which stop at "command not found: pwsh" (Windows PowerShell 5.1 is `powershell`), and the 53 `cd C:\work\demo && …` lines, whose `cd` fails in omp's shell because it reads an unquoted `\` as an escape.
- Inside `pwsh -c`, git runs in PowerShell, outside omp's shell, where the git guard acts; the real guard may differ from the reimplementation there.
- 0497's list doesn't name PowerShell 7.

## Options
| Option | Pros | Cons |
|---|---|---|
| Install PowerShell 7 in the VM | Those 52 lines are measured like the rest | One more install and a 25-minute rerun of the check |
| Keep the reimplementation for them | No install | H1 reports those 52 lines as reimplemented |

## Recommendation
Install PowerShell 7 in the VM.

## Choice
PowerShell 7 (winget's `Microsoft.PowerShell`, the current release) is installed in the VM only, and the git guard check runs again, so the 52 `pwsh -c` lines are measured.

## Reason
"Install PowerShell 7 in the VM".

## Consequences
- Amends 0497: AGENTS.md's install rule lists PowerShell 7.
- The 53 after-`cd` lines stay on the reimplementation; as written they fail at the `cd` in omp's shell, which H1 reports as a fact.
