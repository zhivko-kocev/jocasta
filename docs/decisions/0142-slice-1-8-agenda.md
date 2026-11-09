# 0142 Slice 1.8 agenda

- Status: decided
- Kind: process
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
Which decisions does slice 1.8 (tool overrides) grill, now that v4 P1.5 and P1.7 are decided fresh (0088)?

## Facts
Verified 2026-10-07.
- Pi 1.0.1 has no read-before-edit check, no bash interceptor and no loop guard; it truncates tool output at 2,000 lines or 50 KB (`dist/core/tools/truncate.js:10-11`).
- omp v18.6.0: `edit.mode` defaults to `hashline`, `edit.enforceSeenLines` to true (`packages/coding-agent/src/edit/settings.ts:40,145`); `bashInterceptor.enabled` to false (`src/exec/settings.ts:137`).
- opencode v1.18.22 asks permission after three identical calls (`packages/opencode/src/session/processor.ts:29,356-376`).
- Lesson L15's offline bench, 32 cases: exact 16 correct, 16 refused, 0 wrong; hashline 26, 6, 0; apply_patch 22, 2, 8 wrong (`t3-tools/LESSON-15-edit-benchmark.md:209-214`).
- Lesson L69 prototyped the read guard, the interceptor and a 32 KiB spill as hooks on Pi 1.0.1; a 1 MB result became 170 bytes in context.
- Appendix A rows naming 1.8 include edit style (1.8, 6.11) and the core tools' limits; appendix G assigns edits to unread lines, the loop guard, the interceptor and the spill threshold to 1.8.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of seven: mechanism; edit style; edit-requires-read; bash interceptor; spill; core tool limits; loop guard | Edit style and limits are decided with the guards they interact with | Seven questions |
| The plan's five | Fewer questions | Edit style and limits wait for 6.11 |

## Recommendation
The fresh agenda of seven.

## Choice
The fresh agenda of seven.

## Reason
Not given.

## Consequences
- Plan 1.8's list becomes the seven items, citing this record.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
