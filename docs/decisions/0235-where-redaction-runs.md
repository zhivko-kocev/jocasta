# 0235 Where redaction runs and how it fails

- Status: decided
- Kind: product
- Phase/slice: 2/2.6
- Date: 2026-10-08

## Question
Where does redaction run, how does it fail closed, and what does the marker look like?

## Facts
Verified 2026-10-08 in Pi 1.1.0 (`dist/core/extensions/types.d.ts:1076-1099`): a `tool_result` handler can replace `content`, `details` and `structuredContent`; `message_end` can replace a finished message; `user_bash` can supply the operations that run a `!` command. A throwing `tool_result` handler is skipped and the raw result kept (0078, re-checked 0192).

## Options
| Option | Pros | Cons |
|---|---|---|
| Tool results (`content`, `details`, `structuredContent`) and `!` output; on its own error the handler withholds the result; marker `[REDACTED:<kind>]`; model text not scanned | Small; fails closed despite Pi skipping throwing handlers | A secret the model invents isn't caught |
| Also scan model messages through `message_end` | Catches repeated secrets | More work per message |

## Recommendation
As recommended.

## Choice
Tool results (`content`, `details`, `structuredContent`) and `!` command output are redacted to `[REDACTED:<kind>]` (for example `[REDACTED:env:GITHUB_TOKEN]`). If redaction itself fails, the result is replaced with `result withheld: redaction failed (<error>)`. Model-written text isn't scanned in v1.

## Reason
Not given.

## Consequences
- The model only sees redacted input, so it can only repeat redacted text.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
