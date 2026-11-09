# 0459 Lessons, read at plan and review time, in v1

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
Lessons in compound-engineering form were parked until after v1. Are they built in v1, and how?

## Facts
Verified 2026-10-23.
- ce-compound writes a lesson only when it is non-obvious, durable and material, and `ce-plan` and `ce-code-review` rediscover them (https://github.com/EveryInc/compound-engineering-plugin/blob/main/docs/guides/ce-compound.md).
- Packs: rules with `title` + `applies_when`, cited by the plan, with contradictions flagged in review (https://github.com/EveryInc/compound-engineering-plugin/blob/main/docs/guides/packs.md).
- omp loads extra skill folders from `skills.customDirectories` (`SRC/extensibility/settings.ts:65-69`) and lists each skill with "MUST read `skill://<name>` first" (`SRC/prompts/system/system-prompt.md:42-48`); only its name and description stay in the prompt.
- The overlay every project and thread pane loads is written at `service.ts:163-164`; the thread brief is sent at `service.ts:1757`.
- In-repo `.omp/rules` reach a thread only once committed and merged (`SRC/discovery/builtin.ts:64-68, 103`).
- How omp merges a `skills.customDirectories` array from `--config` with one he sets himself is not verified.

## Options
| Option | Pros | Cons |
|---|---|---|
| Outside the repo, one `lessons` skill per project | Every worktree sees a lesson at once; nothing enters client repos; one prompt line | Plain omp outside Jarvis doesn't see them |
| In the repo as `.omp/rules` | Shared and versioned | Reaches threads only once merged; files in client repos |
| After v1, as parked | No work now | Mistakes repeat |

## Recommendation
Outside the repo, one `lessons` skill per project.

## Choice
Each project's lessons live in `<data>/projects/<pid>/lessons/` (outside client repos), exposed to its panes as one `lessons` skill through `skills.customDirectories` in the per-project overlay. `SKILL.md` is the index (`title — applies when`); each lesson is its own file with `title`, `applies_when`, `date`, `source`, optional `retire_when`. The thread brief tells the thread to read `skill://lessons` before planning and cite the lessons it relies on; the reviewer gets them (0453). A lesson is written only when he says "make that a lesson" (non-obvious, durable, material). Not built: sharing across projects, automatic capture.

## Reason
"Outside the repo, one `lessons` skill per project", in v1.

## Consequences
- Amends the AGENTS.md open item ("lessons in compound-engineering form read at plan time waits until after v1"): it is built in v1.
- The builder checks the `skills.customDirectories` array merge before relying on it.
- The `SKILL.md` template (with how to add a lesson) and the brief line are chosen by the builder and listed as built; plan appendix I names the folder.
- Amended by 0471: after a rejection or a hand-commit on a thread's branch, the boss offers once to save a lesson; saved only on his yes.
- Amended by 0484: `lesson` saves only after he confirms its text in the companion.
