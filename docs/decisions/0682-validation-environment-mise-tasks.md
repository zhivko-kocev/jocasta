# 0682 Validation environment: the project's own mise tasks

- Status: v1
- Kind: process
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
What runs the turn-end and full checks, and does Jocasta provide an environment?

## Facts
- Projects declare tasks in mise (`quick`, `verify`, `test`); Docker Sandboxes (`sbx`) are not installed, run a VM and daemon per sandbox, and omp is not among its supported agents (docs.docker.com/ai/sandboxes, read 2026-11-24).

## Options
| Option | Pros | Cons |
|---|---|---|
| Jocasta starts containers or sandboxes | Isolation | A daemon, memory, install steps |
| The project's own mise tasks `quick` and `verify` | Nothing to install; the project owns its environment | A project needing services starts them from its own task |

## Recommendation
The project's own mise tasks.

## Choice
His answer of 2026-11-24: validation is the project's trusted mise task `quick` (turn end) and `verify` (full), else `test`; Jocasta starts no containers and installs nothing. A project that needs services starts them from its own task. Check depth is defined in 0677.

## Reason
Light, and the project stays in charge.

## Consequences
Jocasta's own repo gets a `quick` task if it lacks one when its code lands in phase 3.
