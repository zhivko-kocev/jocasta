# 0050 Self-upgrade command

- Status: after v1
- Kind: feature
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
Does v1 have a self-upgrade command?

## Facts
Pi's `update` command exists and is never shadowed (plan section 7); a Jarvis self-upgrade needs its own name.

## Options
| Option | Pros | Cons |
|---|---|---|
| After v1 | Re-running the install script works meanwhile | Manual |
| v1 | One command | Claims a command name |

## Recommendation
After v1.

## Choice
After v1.

## Reason
Not given.

## Consequences
Upgrading means re-running the install script.
