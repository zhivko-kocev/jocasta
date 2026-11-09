# 0234 What counts as a secret

- Status: decided
- Kind: product
- Phase/slice: 2/2.6
- Date: 2026-10-08

## Question
What does redaction look for, and where does the list live?

## Facts
- Lean Jarvis (0229): no new config surface without need.

## Options
| Option | Pros | Cons |
|---|---|---|
| Known values plus built-in shapes, compiled in | Catches his real secrets and common token formats | Custom formats outside env and `.env` files are missed |
| The same plus a user pattern list in `permissions.yaml` | Extensible | One more surface |
| Built-in shapes only | Smallest | Misses custom secrets |

## Recommendation
Known values plus built-in shapes.

## Choice
Compiled in, no user list:
- values of env vars whose names contain `TOKEN`, `KEY`, `SECRET` or `PASSWORD`; values in the project's `.env*` files except `.env.example`, `.env.sample` and `.env.template`; tokens in `auth.json`;
- shapes: `ghp_…`, `github_pat_…`, `sk-…`, `sk-ant-…`, `AKIA…`, `xox[bp]-…`, PEM private-key blocks, and credentials in URLs (`https://user:pass@`).

## Reason
Not given.

## Consequences
- None.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
