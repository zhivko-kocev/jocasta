# 0322 Phase 2 as-built texts

- Status: decided
- Kind: product
- Phase/slice: 2/2.1–2.6
- Date: 2026-10-08

## Question
Are phase 2's as-built visible choices accepted?

## Facts
- No record set these; they were explained one by one at his request. He also asked whether the overlay is needed; it was kept (0310).

## Options
| Option | Pros | Cons |
|---|---|---|
| As built | — | — |

## Recommendation
As built.

## Choice
- `jarvis guard rules`: `<action>  <id>  <reason>`; `jarvis guard log`: `<at>  <decision>  <rule>  <tool>: <input>  (<answer>)`.
- A rule from `permissions.yaml` is named `<layer>/<rule text>` with reason `<action> rule in <file>:<line>` (`user` or `project`).
- `version: 1` is optional in `permissions.yaml`.
- The approval overlay is Pi's numbered pick list with 0224's four choices in order.
- Redaction kinds: `env:<NAME>`, `<.env file>:<NAME>`, `auth:<provider>`, `github-token`, `github-pat`, `anthropic-key`, `openai-key`, `aws-key`, `slack-token`, `private-key`, `url-credentials`.
- Known values under 8 characters are ignored; a withheld result is marked as an error; `!` output appears when the command ends.

## Reason
"i need explanation for all"; on the overlay: "for the overlay i am thinking do we need it?"

## Consequences
- None.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
