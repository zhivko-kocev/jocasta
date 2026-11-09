# 0038 Repo layout

- Status: decided
- Kind: product
- Phase/slice: 0/0.1
- Date: 2026-10-04

## Question
How is the repo laid out?

## Facts
`docs/plan.md` and `AGENTS.md` were written on 2026-10-04, before the layout was decided.

## Options
| Option | Pros | Cons |
|---|---|---|
| The plan's preview, plus `docs/plan.md` | Roles separated; shared code has one home | `src/frontier/` is empty until phase 4 |
| The preview without `src/frontier/` until phase 4 | No empty folder | The layout changes at phase 4 |

## Recommendation
The preview.

## Choice
The preview, plus `docs/plan.md`:

```
AGENTS.md                 rules for agents
docs/plan.md              the final plan
docs/decisions/           README.md (index) + NNNN-<slug>.md
src/entry.ts              role dispatch: Frontier or harness
src/harness/              Jarvis's extensions for Pi, one folder per extension
src/frontier/             the orchestrator: server, TUI, boss (phase 4)
src/shared/               projects, threads, storage, config
scripts/build.ts          per-platform compile and packaging
scripts/install.ps1, scripts/install.sh
test/                     unit and smoke tests
```

## Reason
Not given.

## Consequences
Each path appears when the slice that fills it is built; git does not track empty folders.
- Amended by 0383: upstreams are consumed as released; a fork only when a patch is needed, pinned as a submodule.
