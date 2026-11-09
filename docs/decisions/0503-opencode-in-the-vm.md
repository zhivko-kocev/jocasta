# 0503 opencode 1.18.22 is installed in the VM

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-27

## Question
H5 (startup time) and H6 (memory per pane) compare Jocasta with omp, Claude Code and opencode, as the design says. opencode isn't on 0497's list. Is it installed in the VM?

## Facts
Verified 2026-10-27.
- The design's H5 and H6 name opencode; `bench/lib/launch.ts` waits for its ready pattern `ctrl\+p`.
- His PC has opencode 1.18.22 from npm (`opencode-ai`), with an empty `auth.json`: no sign-in.

## Options
| Option | Pros | Cons |
|---|---|---|
| Install opencode 1.18.22 in the VM | The same version as on his PC; startup and idle memory need no sign-in | Its one-prompt memory row runs only if opencode answers without a sign-in |
| Leave opencode out | Nothing to install | H5 and H6 drop a tool the design compares; another change from the registration |

## Recommendation
Install opencode 1.18.22 in the VM.

## Choice
opencode 1.18.22 is installed in the VM from npm, without a sign-in.

## Reason
"Install opencode 1.18.22 in the VM".

## Consequences
- AGENTS.md's install rule lists it.
- H6 reports opencode's one-prompt row only if opencode answers without a sign-in; otherwise the row says why it's missing.
- Amended by 0504: opencode is signed in inside the VM, so H6 measures its one-prompt row.
- Amended by 0506: opencode stays unsigned after all (its Anthropic sign-in takes only an API key), so H6's one-prompt row runs without it, as first planned here.
