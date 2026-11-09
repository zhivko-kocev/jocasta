# 0506 opencode stays unsigned: startup and idle memory only

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-28

## Question
opencode's Anthropic sign-in in the VM takes only an API key: there is no Claude Pro/Max sign-in. 0504 had him sign it in so H6 would measure its one-prompt row. How far does opencode leave H5 (startup time) and H6 (memory)?

## Facts
Verified 2026-10-28.
- At the VM's console, opencode 1.18.22 offered only an API key for Anthropic (his report); `~\.local\share\opencode` holds no `auth.json`.
- H5 and H6's idle row need no sign-in (0503); H6's one-prompt row (`--turn`) sends a prompt.
- His omp accounts and Claude Code run on his subscriptions; an API key is billed per token.
- In Windows PowerShell 5.1 under Windows 11's default execution policy, `opencode` runs npm's `opencode.ps1` and fails ("running scripts is disabled on this system"); `opencode.cmd` runs.

## Options
| Option | Pros | Cons |
|---|---|---|
| Drop opencode from H5 and H6 | The competitors are omp and Claude Code; only its leaving needs explaining | Loses a data point the design registered |
| Keep opencode unsigned for startup and idle memory | 0503's plan: those rows need no sign-in and cost nothing | No one-prompt row for opencode; it may start on a provider screen |
| Sign opencode in with an API key | Every row as registered | Paid per token, a different billing path from the others |

## Recommendation
Keep opencode unsigned for startup and idle memory.

## Choice
opencode stays in the VM without a sign-in. H5 times its startup and H6 measures it idle; H6's one-prompt row has no opencode run, and the results say why.

## Reason
"Keep opencode unsigned for startup and idle memory"; before it, "opencode only supports api key. we can go only with claude code".

## Consequences
- Amends 0504: omp and Claude Code are signed in inside the VM; opencode isn't.
- Amends 0503 as 0504 amended it: H6's one-prompt row runs without opencode, as 0503 first said, and names why.
- Amended by 0537: for the rerun's arm A7 (0543), opencode is signed in inside the VM with an Anthropic API key used for that arm only, with a $150 spend limit; H5 and H6 stay as this record has them.
