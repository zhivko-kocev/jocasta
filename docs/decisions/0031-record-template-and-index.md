# 0031 Record template and index

- Status: decided
- Kind: process
- Phase/slice: 0/0.0
- Date: 2026-10-04

## Question
What template do decision records follow, and how does the index open?

## Facts
- 0013: one Markdown file per decision. 0014: edited in place; status `decided`, feature records `v1`, `after v1` or `dropped`.
- 0015: `NNNN-<slug>.md` in one flat folder. 0016: the index is the feature register, with Kind and Status columns.
- The folder path is decided in 0.1 (0038: `docs/decisions/`).

## Options
| Option | Pros | Cons |
|---|---|---|
| The plan's preview: header lines, then Question, Facts, Options, Recommendation, Choice, Reason, Consequences; the index opens with four lines, then a six-column table | Keeps the evidence each grilling question carries; Consequences records where the choice differs from the recommendation (0004) | More to write; back-filled records 0001–0030 can't reconstruct their planning-era facts and options |
| Lighter: Question, Choice, Reason, Consequences | Fast to write and read | Loses facts and rejected options, so re-opening a decision (0004) has no baseline |

## Recommendation
The plan's preview.

## Choice
The plan's preview.

```markdown
# NNNN Title

- Status: decided        <!-- features: v1 | after v1 | dropped -->
- Kind: process | product | feature
- Phase/slice: 0/0.0     <!-- `planning` for decisions made before phase 0 -->
- Vision: V9             <!-- the V-numbers this rests on, if any -->
- Date: 2026-10-02       <!-- date of the current choice; updated on edit -->

## Question
## Facts
## Options
| Option | Pros | Cons |
## Recommendation
## Choice
## Reason             <!-- Zhivko's words when he gave them -->
## Consequences       <!-- what changes in the build; how the choice differs from the recommendation -->
```

The index (`README.md` in this folder) opens with four lines: what the log is; that the vision image is the source of truth (0019); that records are edited in place, with history in git; that the index is also the feature register, and requests after v1 are GitHub issues. Then the table `# | Decision | Kind | Status | Phase/slice | Date`.

## Reason
Not given.

## Consequences
- Records 0001–0030 are back-filled from the plan; their Facts and Options point to the v4 and v5 plans.
- `Phase/slice: planning` marks decisions made before phase 0 opened.
