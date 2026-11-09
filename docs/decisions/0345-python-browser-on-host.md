# 0345 The python and browser tools run on the host

- Status: decided
- Kind: product
- Phase/slice: 6/6.8 (amends 0070 for 6.8)
- Date: 2026-10-08

## Question
Do 6.8's python and browser tools run in containers (0070) or on Windows?

## Facts
- 0070: every headless resource behind a port or stdio runs in a container; plan 0.7 names "6.8 eval runners and the browser" among them.
- 0289 chose a persistent `python` tool and an Edge browser tool over CDP, noting Python 3.14.2 and Edge are installed on the host.
- Repo files mounted from `D:\` into WSL 2 containers are much slower than native access (plan 0.7). Edge has no Linux container image; a container would use Chromium.

## Options
| Option | Pros | Cons |
|---|---|---|
| On Windows | Sees his files and installed packages; the real Edge; fast; no images | Not isolated beyond the guard |
| In containers | Isolated | Slower repo access; Chromium instead of Edge; 1–2 GB of images |
| Python on the host, the browser in a container | — | Mixed |

## Recommendation
On Windows.

## Choice
On Windows. Both tools go through the guard.

## Reason
Chosen from the options.

## Consequences
- 0070 no longer covers 6.8; plan 0.7's list drops "6.8 eval runners and the browser".
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
