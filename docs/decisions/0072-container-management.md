# 0072 Container management

- Status: decided
- Kind: product
- Phase/slice: 0/0.7
- Date: 2026-10-04

## Question
How are containers managed?

## Facts
Principle 7: one module per external tool.

## Options
| Option | Pros | Cons |
|---|---|---|
| One Compose file in the config dir, owned by one Jarvis module, with health checks and restart policies | One place | — |
| Each resource on its own | No shared file | Breaks principle 7 |

## Recommendation
One Compose file.

## Choice
One Compose file, `~/.jarvis/agent/compose.yaml`, owned by one Jarvis module, with health checks and restart policies.

## Reason
Not given.

## Consequences
The config dir table (0055) gains `compose.yaml`, edited by Jarvis.
