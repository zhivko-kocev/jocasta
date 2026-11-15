# 0635 saphyr reads and writes YAML

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
Which YAML crate does the Rust build use?

## Facts
Verified 2026-11-15. Crate counts are transitive, on Windows, on top of the base of 0631.
- **What Jocasta does with YAML:**
  - It reads his config files and its shipped ones.
  - It writes only two files, both of them its own generated files: the omp settings overlay (`src/setup/omp-settings.ts:77`) and setup's moved config (`src/setup/setup.ts:305`).
  - It edits his files line by line, without a YAML library (`projects-file.ts`, `config-file.ts`), and keeps doing so.
- **Comments.** Only yaml-edit keeps comments, and it is young and untried.
- **The candidates:**
  - yaml-rust2 0.13 (+5): YAML 1.2, passes the full test suite, has a loader and an emitter. Its README says it gets "only basic maintenance" and that new features go to saphyr.
  - saphyr 0.3.4 (+11): the successor by the same team, released 4 days ago.
  - serde_norway 0.9.42 (+3): last released 693 days ago.
  - serde_yaml is deprecated, and serde_yml has an advisory as unsound and unmaintained.

## Options
| Option | For | Against |
|---|---|---|
| yaml-rust2 | fewest crates, stable API | basic maintenance only |
| saphyr | active | twice the crates; its API still moves |
| serde_norway | typed structs | stale |

## Recommendation
yaml-rust2.

## Choice
saphyr 0.3 reads YAML and writes the two generated files. His files are still edited line by line.

## Reason
His answer: `saphyr`.

## Consequences
- This differs from the recommendation.
- **The generated files' bytes** may differ from what `Bun.YAML` wrote. omp reads either form. The port's tests show any difference.
