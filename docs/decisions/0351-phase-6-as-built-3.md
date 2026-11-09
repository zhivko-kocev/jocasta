# 0351 Phase 6 as-built, last items

- Status: decided
- Kind: product
- Phase/slice: 6/6.4, 6.9
- Date: 2026-10-08

## Question
Are the last phase 6 choices accepted?

## Facts
Verified 2026-10-08 in omp @89d2610 (`D:/learn-harnesses/repos/oh-my-pi`):
- omp's `task` tool is exec-level, and its approval details show each agent and task (`task/index.ts:565-583`).
- omp files `wait` in its "read" approval tier, which never asks (`tools/wait.ts`, `task/read-only-policy.ts`).
- omp counts an agent as read-only only when every declared tool is in the read tier, so no shell (`task/read-only-policy.ts`); its plan-mode subagent prompt says read-only operations only, explore and report (`prompts/system/plan-mode-subagent.md`).
- billion-context's proxy adds its context tool to any Messages request with tools; its Pi plugin prints `bili-plugin(pi): subagent self-registration skipped (path-invalid)` on every run, for a third-party extension Jarvis doesn't use.
- The phase 6 exit smoke passed through the compiled `jarvis.exe`: two scout subagents returned schema-valid results; an edit adding a Go type error showed gopls's error in the result; a new workspace thread stopped at its plan with exit 3.

## Options
| Option | Pros | Cons |
|---|---|---|
| Accept, with the shell on the safe list for read-only agents | Scouts can run `git log` and tests while planning | Looser than omp |
| Read-only means no shell, as in omp | Strictest | Scouts lose git and test commands |

## Recommendation
Accept, shell on the safe list.

## Choice
- In a normal tab the `subagent` call's card shows `subagent: N tasks` and one `agent: task` line per task, cut at 300 characters; an `allow` rule for `subagent` skips it.
- `wait` and `send` never ask.
- A read-only child is told: "You were started while a plan is still being written, or from a thread that changes nothing. Explore and report what you find; don't change files or run commands that change anything: such calls are denied." Its denial reads "read-only subagent: explore and report; nothing changes until the plan is approved or in a workspace thread". Read-only agents keep their shell, limited to 0238's list.
- `web_search`'s Anthropic request skips billion-context's proxy, still shaped for the subscription.
- Jarvis hides billion-context's `bili-plugin(pi): subagent self-registration skipped (path-invalid)` line.

## Reason
"explain me the points and see how other harnesses handle them"; then chosen.

## Consequences
- AGENTS.md's open item about the bili-plugin message is closed.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
