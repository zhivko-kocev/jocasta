# 0146 Edit style

- Status: decided
- Kind: product
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
Which edit tool does the model get?

## Facts
Verified 2026-10-07.
- Pi 1.0.1's `edit` is exact replace (`oldText`, `newText`), with no fuzzy matching.
- Lesson L15's offline bench, 32 copy-error cases, run on this machine: exact 16 correct, 16 refused, 0 wrong; replacer chain 22, 10, 0; hashline 26, 6, 0 (31, 1, 0 with stale recovery); `apply_patch` 22, 2, 8 wrong (`t3-tools/LESSON-15-edit-benchmark.md:209-214,318-324`).
- omp v18.6.0 defaults `edit.mode` to `hashline` (`packages/coding-agent/src/edit/settings.ts:40`); his 4,898 omp edit calls (appendix A) ran under it.
- Hashline (lesson L14): reads tag each line with a short hash; edits cite line numbers and tags, so a stale read is refused exactly.
- What changed (0004): the row's first option was exact replace; the bench and his usage favor hashline.

## Options
| Option | Pros | Cons |
|---|---|---|
| Hashline | Most correct edits; never a wrong apply; stale reads caught | Jarvis owns its read and edit tools; reads cost a few characters per line |
| Exact replace (Pi's) | No new code | Refuses half the drift cases |
| Replacer chain | Six more correct than exact, 0 wrong; Pi's schema | Tolerance conflicts with the "drifted edits fail loudly" lever |
| `apply_patch` | Codex-style diffs | 8 wrong applies in 32 |

## Recommendation
Hashline. This changes the row's first option because of the bench and his usage.

## Choice
Hashline: Jarvis's `read` shows each line with its number and a 4-hex tag, and Jarvis's `edit` replaces, inserts or deletes by line number and tag, refusing when a tag no longer matches.

## Reason
Not given.

## Consequences
- Jarvis replaces Pi's `read` and `edit` by name (0147).
- Preview: `1#a3f2: export function add(a, b) {` and `REPLACE 2#9c1e..2#9c1e`; the exact syntax and the tag hash are decided in the build's tool descriptions, measured by the 1.10 evals.
- 1.12's drifted-edits lever is met: hashline never matches text.
- Appendix A's edit-style row (1.8, 6.11) is decided here.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
