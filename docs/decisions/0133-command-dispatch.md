# 0133 How Jarvis's commands reach the CLI

- Status: decided
- Kind: product
- Phase/slice: 1/1.6
- Date: 2026-10-07

## Question
How do `jarvis project`, `jarvis thread` and later commands reach the CLI?

## Facts
Verified 2026-10-07 in Pi 1.0.1:
- Any unknown positional argument becomes a prompt (`dist/cli/args.js:250-251`), so `jarvis project list` would go to the model.
- Pi claims first-arg commands in `main()` before parsing: `install`, `remove`, `update`, `list`, `config`, `mcp` and `auth` (`dist/main.js:469-487`, `dist/package-manager-cli.js:203-212`, `dist/cli/auth-command.js:16`).
- Extensions register flags and slash commands; they can't add subcommands. This answers plan 5.4's phase 1 item.
- 0123 and 0132 already use `/thread new` inside the TUI.
- What changed (0004): the in-TUI slash commands were decided before this question, so "both" replaces the plan's first option.

## Options
| Option | Pros | Cons |
|---|---|---|
| Both: the entry dispatches `jarvis <command>` before Pi's `main()`, and a Jarvis extension registers slash commands | Scripts and the boss use the CLI; he stays in the TUI | Two front doors per command, one implementation |
| The entry only | One front door | Contradicts 0123 and 0132 |
| Slash commands only | All inside Pi's extension API | No scripting; CLI words go to the model |

## Recommendation
Both.

## Choice
Both. `src/entry.ts` dispatches Jarvis's commands before Pi's `main()`; a Jarvis extension registers the matching slash commands (`/thread …`) in the TUI. Both call one implementation.

## Reason
Not given.

## Consequences
- Appendix I's seam "Jarvis's command dispatch (1.6)" is this.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
