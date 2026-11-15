# 0622 Tuning program: two small calls from the factory view

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-15

## Question
Two small calls were left from the factory view (`docs/research/state-of-the-art.md`, section 14). They were asked as ticks (0089), with the thinking arm that 0620 records. Which are taken?

## Facts
Verified 2026-11-15.
- **The records as a graph:** 616 record files hold 450 "amends" links to 227 records. For 107 of those 227, neither the record's own text nor its index row names every record that amended it. 0598's row does not say that 0600 turned the git guard on again. `AGENTS.md` sends an agent to the index when a task touches a decision (0616).
- **`test/docs/`** (0608) already checks that the index equals the record files.
- **The design debts** D5, D6 and D7 of the research (section 11): the service's forwarding methods, two readers of omp's cost, route files with one consumer. 0610's tree checks hold today's count as a baseline. Run over the same 207 session files, the two cost readers agree to the cent ($1,662.15 each).
- **The benchmarks** measure the product as built; a refactor before them changes what they measure.

## Options
| Call | Options | Recommended |
|---|---|---|
| What amended a record | An "Amended by" column in the index, checked by a test; as it is | The column |
| The design debts | One refactoring slice after the benchmarks; before them; not at all | After |

## Recommendation
Both as in the table's last column.

## Choice
- **The index gains an "Amended by" column.** A test in `test/docs/` computes it from the records' own "amends" lines and fails when a row lacks a record that amended it.
- **D5, D6 and D7 are one refactoring slice after the benchmarks,** with the scorecard (0616) as its before and after. It changes no behaviour.

## Reason
His ticks.

## Consequences
- Amends 0031 (the index's columns) and 0608 (the docs checks).
- A new record that amends another fails the check until the amended record's row names it.
- The plan's own decisions table (section 3) is not changed by this record.
