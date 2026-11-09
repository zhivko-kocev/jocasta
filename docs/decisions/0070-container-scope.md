# 0070 What runs in containers

- Status: decided
- Kind: product
- Phase/slice: 0/0.7
- Date: 2026-10-04

## Question
Which resources run in containers (0030)?

## Facts
- Containers on Windows run inside WSL 2; bind-mounting repos from `D:\` is much slower than native access.
- Host-only either way: Jarvis and its tabs, mic and speakers, credentials, repo toolchains and language servers.

## Options
| Option | Pros | Cons |
|---|---|---|
| Every headless resource behind a port or stdio | Most isolation; one way to run things | Repo-heavy work hits the slow mount |
| Services only; eval runners and the browser on the host | Isolation where cheap | Two ways |
| Nothing | Simplest | Reverses 0030 |

## Recommendation
Every headless resource.

## Choice
Every headless resource behind a port or stdio: speech, MCP servers, databases, eval runners, the browser, rehearsals and scanners.

## Reason
Not given.

## Consequences
A slice that works on repo files at speed can ask for a host exception in its own grilling.
