# 0131 Thread session storage

- Status: decided
- Kind: product
- Phase/slice: 1/1.5
- Date: 2026-10-07

## Question
Where do each thread's internal Pi session files live?

## Facts
- The data dir is the OS data dir (0095); one `project.db` per project (0096); the home thread lives in `catalog.db` (0125).
- Lesson L20: a thread is a sequence of Pi session files, one per context window, each an append-only tree.

## Options
| Option | Pros | Cons |
|---|---|---|
| Per thread under the project: `<data>/projects/<pid>/threads/<tid>/`, and `<data>/home/<tid>/` for the home thread | A project is one folder; a thread's windows sit together for history search | Deeper paths |
| One dir per project, tagged in `project.db` | Shallower | Membership lives only in the DB |
| Pi's default dir plus an index | Nothing to change | Session files stay in the config dir |

## Recommendation
Per thread under the project.

## Choice
`<data>/projects/<pid>/threads/<tid>/` for project and workspace threads, `<data>/home/<tid>/` for the home thread; the entry passes it as `--session-dir`.

## Reason
Not given.

## Consequences
- Phase 0's `<config dir>/sessions/` is not migrated (0057).
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
