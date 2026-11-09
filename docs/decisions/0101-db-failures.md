# 0101 DB failures

- Status: decided
- Kind: product
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
What happens when a DB can't be opened, was written by a newer Jarvis, or stays locked past `busy_timeout`?

## Facts
- Principle 2: loud failures instead of fuzzy recovery. Principle 4: fail closed on anything that guards; phase 2 keeps guard grants and audit in these DBs.
- Exit codes are 1.7's decision; appendix I proposes 1 for "the run failed".

## Options
| Option | Pros | Cons |
|---|---|---|
| Stop with a message naming the file, the cause and the fix; exit 1; never delete, recreate or downgrade the file | No silent data loss | A corrupt `project.db` blocks that project until he acts |
| Newer and locked stop as above; a corrupt file is renamed to `project.db.corrupt-<time>` and a new one starts empty | Never blocks work | Threads and grants vanish until he notices; bends principle 2 |

## Recommendation
Stop loud, exit 1.

## Choice
Stop with a one-line message naming the file, the cause and the fix, and exit 1. Jarvis never deletes, recreates or downgrades a DB file.

## Reason
Not given.

## Consequences
- Messages, as previewed:
  - `jarvis: <path>\catalog.db is schema 7; this Jarvis knows up to 5. Update Jarvis.`
  - `jarvis: <path>\project.db can't be opened: file is not a database. It was left as is.`
  - `jarvis: <path>\catalog.db is locked by another Jarvis process (waited 5 s).`
- Appendix I's exit codes: 1 is used for these failures; 1.7 keeps or replaces the set.
