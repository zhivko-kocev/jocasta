# 0071 Container runtime

- Status: decided
- Kind: product
- Phase/slice: 0/0.7
- Date: 2026-10-04

## Question
Which container runtime?

## Facts
- This machine, 2026-10-04: Docker Desktop installed and running (`docker` 29.5.2; WSL distros `docker-desktop` running, `Ubuntu` stopped); Podman not installed.
- Docker Desktop is free for personal use, education and small businesses (under 250 employees and under $10 million revenue); professional use in larger organizations needs a paid subscription (verified 2026-10-03, plan 0.7).

## Options
| Option | Pros | Cons |
|---|---|---|
| Docker Desktop | Already installed | Licensing for large-employer work |
| Docker Engine inside WSL 2 | Open source | Install; two daemons |
| Podman | Daemonless | Not installed |

## Recommendation
Docker Desktop.

## Choice
Docker Desktop.

## Reason
Not given.

## Consequences
Licensing follows Docker's terms for how the machine is used.
