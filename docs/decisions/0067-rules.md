# 0067 Rules

- Status: decided
- Kind: product
- Phase/slice: 0/0.6
- Date: 2026-10-04

## Question
Which standing rules does Jarvis follow?

## Facts
- The omp plan's rule files aren't on this machine.
- The security rules must agree with phase 2's guard; phase 2 checks that.

## Options
| Option | Pros | Cons |
|---|---|---|
| Approve the preview: communication, development, research, security | Covers the plan's rule areas except workflow | — |
| Change it | — | — |

## Recommendation
Approve.

## Choice
The rules in `~/.jarvis/agent/AGENTS.md`: English and terse; read before editing; run the project's checks; never commit, push, merge, rebase or delete branches unless asked; verify against primary sources; never expose secrets; destructive commands only on explicit request.

## Reason
Not given.

## Consequences
- The workflow rules (his week; infer mode from repo and clock) are left out until the identity describes his week.
- Phase 2 checks the security rules against the guard.
