# 0172 Jarvis's prompt section

- Status: decided
- Kind: product
- Phase/slice: 1/1.11
- Date: 2026-10-07

## Question
What does Jarvis add to Pi's system prompt?

## Facts
- The text ships in the binary; identity and persona stay in `~/.jarvis/agent` (0066–0069).
- 0167 hillclimbs this section later.

## Options
| Option | Pros | Cons |
|---|---|---|
| The previewed section, about 110 words of facts the model can't see otherwise | Threads, artifacts and hashline conventions are known from turn one | — |
| Pi's prompt only | No shipped text | Conventions only from tool descriptions |

## Recommendation
The previewed section.

## Choice
Now (0381): the tool-use line names only the active ones of read, grep, find and ls, and is omitted when none is active; the rest stands.

Earlier (0314): the section also carries the self-extension line of 0314.

Originally: ```
# Jarvis
You run inside Jarvis. Work is organized in threads, not sessions:
- This conversation is the {kind} thread "{title}" of project {project}.
- The project thread lives on trunk and is chat-only. Code changes happen in
  workspace threads, each on its own branch and worktree; propose one with a
  title and a brief when work is needed.
- Tool output over 32 KiB is saved as artifact://<n>; read it with ranges.
- local://<name>.md files are this thread's scratch space.
- read shows line tags; edit cites line numbers and tags from your latest read.
- Use the read, grep, find and ls tools, not shell equivalents.
```

## Reason
Not given.

## Consequences
- The home thread fills `{kind}` as `home` and has no project line.
- Amended by 0314: one line added.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
