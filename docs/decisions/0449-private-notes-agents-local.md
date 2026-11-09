# 0449 Private project notes through AGENTS.local.md

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
Hand-kept project notes (0400) reach a thread worktree only once committed. How do private notes reach every thread of a project?

## Facts
Verified 2026-10-23.
- `<repo>/.omp/AGENTS.md` reaches a thread worktree only once committed: omp walks ancestors only up to the worktree's root (`SRC/discovery/builtin.ts:103, 959`), and worktrees sit beside the project (0445).
- omp reads `contextFiles.extra` names from the main checkout for a linked worktree (`discovery/custom-context-files.ts:74-92`; omp `docs/context-files.md:87-111`).
- No Jarvis code carries notes into worktrees (grep `src/`: none).

## Options
| Option | Pros | Cons |
|---|---|---|
| `contextFiles.extra: [AGENTS.local.md]` in the overlay | Reaches every thread without a commit; nothing enters client repos | One gitignored file per project |
| Commit `.omp/AGENTS.md` | Plain omp sees it too | Private notes enter client repos (principle 6) |
| Jarvis copies notes into each worktree | Any file name | Jarvis code; copies drift |

## Recommendation
`contextFiles.extra: [AGENTS.local.md]` in the overlay.

## Choice
The per-project overlay sets `contextFiles.extra: [AGENTS.local.md]`; an `AGENTS.local.md` at the project's root (gitignored, his) reaches every thread of that project without being committed. `<repo>/.omp/AGENTS.md` keeps working as before.

## Reason
"`contextFiles.extra: [AGENTS.local.md]` in the overlay".

## Consequences
- Amends 0400 (where hand-kept notes can live).
- Plan appendix I lists the overlay key and the file name.
